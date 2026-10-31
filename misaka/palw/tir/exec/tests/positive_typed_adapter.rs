#![cfg(feature = "positive-panel")]
//! Source-only typed-capture→legacy/source-material adapter integration.
//! The original sparse class declaration is unchanged. A separate NEW range-
//! admissible declaration is used where the existing exact source evaluator
//! requires it. Neither fixture asserts native registration or acceptance.
mod positive_typed_adapter_common;
mod positive_typed_capture_common;

use kaspa_consensus_core::{
    Hash64,
    palw_artifact::PalwArtifactOpeningV1,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_positive_check_v2::{
        CheckOutcome,
        expanded_plan::{ExpandedPlan, ExpandedPlanLimits, ExpandedRelation, ExpandedRelationKind},
        full_relation::{FullRelationError, FullRelationLimits, FullRelationPreparation, LegacyTarget, PrototypeDomains},
    },
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, PalwPromptIdsOpeningV1},
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_merkle_range_siblings_v1, step_opening_v1},
    palw_step_refute::{PALW_LOGITS_TILE_LANES, PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1, tiled_logits_rows_root_v1},
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::{
        PalwTirCourtRulesV1, PalwTirEvidenceStoreV1, PalwTirTraceEventDisclosureV1, source_only::TirSourceOnlyLimitsV1,
        tir_logits_event_disclosure_v1,
    },
    palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepSpaceV1},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{CONTRACT_REVISION, CONTRACT_VERSION, identity::*, messages::*, tir::greedy_selector_id};
use misaka_palw_tir::{DType, Prim, Tensor, demand::DemandLimits};
use misaka_palw_tir_exec::node::{
    TirParamOpenerV1,
    positive_typed_adapter::{
        StageReservations, TypedAdapterError, TypedAdapterExpected, TypedAdapterLimits, TypedCaptureAdapter, quote_typed_adapter,
    },
    positive_typed_capture::{TypedCaptureLimits, capture_typed_prototype},
};

const EXPECTED: &[u32] = &[0, 1, 0];
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn digest(n: Hash64) -> Digest {
    Digest(n.as_bytes())
}
fn capture_limits() -> TypedCaptureLimits {
    TypedCaptureLimits {
        max_positions: 16,
        max_geometry_visits: 1 << 20,
        max_records: 1 << 20,
        max_lanes: 1 << 22,
        max_payload_bytes: 1 << 26,
        max_peak_capture_heap_bytes: 1 << 28,
        max_local_storage_bytes: 1 << 28,
        max_legacy_leaves: 1 << 16,
    }
}
fn adapter_limits() -> TypedAdapterLimits {
    TypedAdapterLimits {
        max_index_records: 1 << 20,
        max_index_bytes: 1 << 28,
        max_lookup_visits: 1 << 38,
        max_proof_attempts: 1 << 28,
        max_proof_leaf_reads: 1 << 38,
        max_hash_work: 1 << 42,
        max_legacy_leaves: 1 << 16,
        max_tile_lanes: 1 << 16,
        max_legacy_payload_bytes: 1 << 20,
        max_path_siblings: 64,
        max_claims_heap_bytes: 1 << 28,
        max_material_bytes: 1 << 50,
        max_additional_heap_bytes: 1 << 34,
    }
}
fn full_limits() -> FullRelationLimits {
    FullRelationLimits {
        plan: ExpandedPlanLimits {
            max_positions: 16,
            max_geometry_visits: 1 << 24,
            max_plan_bytes: 1 << 24,
            max_legacy_leaves: 1 << 16,
            max_relations: 1 << 20,
            max_target_lanes: 1 << 26,
            max_peak_target_bytes: 1 << 20,
            uncommitted_tile_lanes: 3,
            max_elements_per_relation: 1 << 14,
            max_terms_per_relation: 1 << 16,
            max_total_elements: 1 << 52,
            max_total_terms: 1 << 52,
        },
        source: TirSourceOnlyLimitsV1 {
            max_auth_storage_bytes: 1 << 28,
            max_workspace_bytes: 1 << 30,
            max_source_reads: 1 << 16,
            max_target_lanes: 1 << 16,
            max_target_bytes: 1 << 20,
            max_material_bytes: 1 << 24,
            max_material_units: 1 << 16,
        },
        max_record_bytes: 1 << 28,
        max_record_heap_bytes: 1 << 28,
        max_record_geometry_visits: 1 << 52,
        max_total_material_bytes: 1 << 44,
        max_total_source_reads: 1 << 40,
    }
}
fn stages() -> StageReservations {
    StageReservations {
        retained_preparation_bytes: 1 << 26,
        source_auth_bytes: 1 << 28,
        evaluator_bytes: 1 << 30,
        signature_workspace_bytes: 1 << 20,
    }
}
fn domains() -> PrototypeDomains {
    PrototypeDomains::local_only(b"A1/full-relation/local-test-record", b"A1/FULL/LOCAL/TEST").unwrap()
}

trait OracleFixture {
    fn class(&self) -> &PalwTirClassV1;
    fn job(&self) -> &PalwJobContextV2;
    fn artifact(&self) -> Hash64;
    fn node(&self, position: u32, occurrence: u16, node: u16) -> &Tensor;
    fn state(&self, position: u32, state: u16, layer: Option<u16>) -> &Tensor;
    fn appended(&self, position: u32, state: u16, layer: Option<u16>) -> Vec<i128>;
}
macro_rules! oracle_fixture {
    ($name:ty)=>{impl OracleFixture for $name {
        fn class(&self)->&PalwTirClassV1 {&self.class}
        fn job(&self)->&PalwJobContextV2 {&self.job}
        fn artifact(&self)->Hash64 {self.artifact}
        fn node(&self,position:u32,occurrence:u16,node:u16)->&Tensor {&self.nodes.iter().find(|n|n.position==position&&n.occurrence==u32::from(occurrence)&&n.node==node).unwrap().value}
        fn state(&self,position:u32,state:u16,layer:Option<u16>)->&Tensor {&self.steps[position as usize].state.fixed[&(state,layer)]}
        fn appended(&self,position:u32,state:u16,layer:Option<u16>)->Vec<i128> {
            let node=self.nodes.iter().find(|n|n.position==position&&n.layer==layer&&matches!(self.program.blocks[n.block as usize].nodes[n.node as usize].prim,Prim::HistAppend {state:s} if s==state)).unwrap();
            let lanes=self.program.states[state as usize].shape.iter().map(|n|*n as usize).product::<usize>();
            node.value.data[node.value.data.len()-lanes..].to_vec()
        }
    }};
}
oracle_fixture!(positive_typed_capture_common::Fixture);
oracle_fixture!(positive_typed_adapter_common::Fixture);
fn oracle_claim(f: &impl OracleFixture, relation: &ExpandedRelation) -> Vec<i128> {
    match &relation.kind {
        ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
            f.node(ctx.pos, ctx.occurrence, *node).data[*first_lane as usize..*first_lane as usize + *lane_count as usize].to_vec()
        }
        ExpandedRelationKind::Legacy(leaf) => match leaf.kind {
            PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => f.node(leaf.position, occurrence as u16, node).data
                [first_element as usize..first_element as usize + leaf.value_count as usize]
                .to_vec(),
            PalwTirLeafKindV1::State { state, layer, first_element, .. } => f.state(leaf.position, state, layer).data
                [first_element as usize..first_element as usize + leaf.value_count as usize]
                .to_vec(),
            PalwTirLeafKindV1::HistTile { state, layer, first_lane, row_lanes, first_position, .. } => {
                let mut values = Vec::new();
                for position in first_position..=leaf.position {
                    let row = f.appended(position, state, layer);
                    values.extend_from_slice(&row[first_lane as usize..first_lane as usize + row_lanes as usize]);
                }
                values
            }
        },
    }
}
struct SignedMiner {
    rooted: AdmissionContext,
    result: MinerResultCommit,
    key: [u8; PUBKEY_BYTES],
    signature: [u8; SIGNATURE_BYTES],
}
impl SignedMiner {
    fn new(f: &impl OracleFixture) -> Self {
        let class = f.class();
        let job = f.job();
        let artifact = f.artifact();
        assert_eq!(class.tokenizer_id, Hash64::from_u64_word(18));
        assert_eq!(job.tokenizer_id, Hash64::from_u64_word(0));
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
        let key = *pair.verification_key.as_ref();
        let rooted = AdmissionContext {
            scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
            expected_job: digest(job.job_id),
            expected_execution: digest(job.context_hash()),
            expected_miner: principal(&key),
            expected_input: digest(job.prompt_token_ids_hash),
            registered_class: digest(class.class_id(&artifact)),
            artifact: digest(artifact),
            program: digest(class.graph_ir_root()),
            tokenizer: digest(job.tokenizer_id),
            execution_rules: digest(job.cu_ruleset_id),
            selector: greedy_selector_id(),
            required_form: ClaimForm::PositiveResultV2,
        };
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
            output: digest(palw_attempt_output_root_v1(job, EXPECTED)),
            dependencies: d(21),
            service_contract: d(22),
        };
        let signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        Self { rooted, result, key, signature }
    }
    fn authenticate(&self) -> AuthenticatedResult<'_> {
        self.result.authenticate(&self.key, &self.signature).unwrap()
    }
    fn preparation(&self, f: &impl OracleFixture) -> FullRelationPreparation {
        FullRelationPreparation::authenticate(
            &self.rooted,
            f.class(),
            f.artifact(),
            f.job(),
            PalwPromptIdsFormV1::Flat,
            full_limits(),
            domains(),
            self.result.clone(),
            &self.key,
            &self.signature,
        )
        .unwrap()
    }
}

struct Sources<'a> {
    inventory: &'a dyn TirParamOpenerV1,
    prompt: &'a [u32],
    pin: PalwDecodeTokenPinV1,
}
impl PalwTirEvidenceStoreV1 for Sources<'_> {
    fn begin_builder_move_v1(&self) {
        panic!("typed adapter must not delegate a source budget reset hook")
    }
    fn step_leaf(&self, _: u64) -> Option<PalwStepTileLeafV1> {
        panic!("typed adapter must not delegate step preimages or replay")
    }
    fn step_opening(&self, _: u64) -> Option<PalwStepOpeningV1> {
        panic!("typed adapter must not delegate step openings")
    }
    fn step_range_siblings(&self, _: u64, _: u64) -> Option<Vec<Hash64>> {
        panic!("typed adapter must not delegate step ranges")
    }
    fn param_opening(&self, index: u32) -> Option<PalwArtifactOpeningV1> {
        self.inventory.param_opening(index)
    }
    fn prompt_token_ids(&self) -> Option<Vec<u32>> {
        Some(self.prompt.to_vec())
    }
    fn prompt_ids_opening(&self, _: u32) -> Option<PalwPromptIdsOpeningV1> {
        None
    }
    fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1> {
        Some(self.pin.clone())
    }
}
fn sources<'a>(
    inventory: &'a dyn TirParamOpenerV1,
    prompt: &'a [u32],
    binding: &PalwTirStepBindingV1,
    run: &misaka_palw_tir_exec::node::TirJobRunV1,
) -> Sources<'a> {
    Sources {
        inventory,
        prompt,
        pin: PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 {
            rows_root: tiled_logits_rows_root_v1(&binding.job_context, &run.logits_rows).unwrap(),
            generated_token_ids: run.generated.clone(),
        }),
    }
}
struct OwnedLegacy {
    preimage: PalwStepTileLeafV1,
    opening: PalwStepOpeningV1,
    trace: Option<PalwTirTraceEventDisclosureV1>,
}
impl OwnedLegacy {
    fn borrowed(&self) -> LegacyTarget<'_> {
        LegacyTarget { preimage: &self.preimage, opening: &self.opening, trace: self.trace.as_ref() }
    }
}

fn legacy_for(
    adapter: &TypedCaptureAdapter<'_>,
    relation: &ExpandedRelation,
    run: &misaka_palw_tir_exec::node::TirJobRunV1,
) -> Option<OwnedLegacy> {
    let ExpandedRelationKind::Legacy(leaf) = relation.kind else { return None };
    let binding = adapter.binding().unwrap();
    let program = binding.class.decode_program().unwrap();
    let space = PalwTirStepSpaceV1::new(&binding.class).unwrap();
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
                    (first_element / PALW_LOGITS_TILE_LANES as u64) as u8,
                )
                .unwrap(),
            )
        }
        _ => None,
    };
    Some(OwnedLegacy {
        preimage: adapter.legacy_preimage(leaf.index).unwrap(),
        opening: adapter.legacy_opening(leaf.index).unwrap(),
        trace,
    })
}
fn rules() -> PalwTirCourtRulesV1 {
    let limits = full_limits();
    PalwTirCourtRulesV1 {
        max_step_leaf_count: limits.plan.max_legacy_leaves,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: limits.plan.max_elements_per_relation, max_terms: limits.plan.max_terms_per_relation },
    }
}

#[test]
fn original_sparse_capture_reconstructs_every_legacy_preimage_opening_range_and_expanded_lane() {
    for window in [1, 3] {
        positive_typed_capture_common::with_window_fixture(window, |f, runner, _| {
            let signed = SignedMiner::new(f);
            let authenticated = signed.authenticate();
            let capture = capture_typed_prototype(
                runner,
                &f.class,
                f.artifact,
                &f.job,
                f.job.context_hash(),
                positive_typed_capture_common::PROMPT,
                capture_limits(),
            )
            .unwrap();
            let adapter = TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &authenticated,
                    rooted: &signed.rooted,
                    step_space: runner.space,
                },
                adapter_limits(),
            )
            .unwrap();
            let mut leaves = Vec::new();
            let original = runner
                .run(&f.job, positive_typed_capture_common::PROMPT, 1 << 16, true, &mut |leaf| leaves.push(leaf.preimage.clone()))
                .unwrap();
            assert_eq!(capture.run(), &original);
            for index in 0..original.leaf_count {
                let preimage = adapter.legacy_preimage(index).unwrap();
                let opening = adapter.legacy_opening(index).unwrap();
                assert_eq!(preimage, leaves[index as usize]);
                assert_eq!(opening, step_opening_v1(&original.leaf_hashes, index).unwrap());
                adapter.validate_legacy(index, &preimage, &opening).unwrap();
                assert_eq!(borsh::to_vec(&preimage).unwrap(), borsh::to_vec(&leaves[index as usize]).unwrap());
            }
            for (first, count) in [(0, 1), (0, original.leaf_count), (original.leaf_count / 2, 2), (original.leaf_count - 1, 1)] {
                assert_eq!(
                    adapter.legacy_range_siblings(first, count).unwrap(),
                    step_merkle_range_siblings_v1(&original.leaf_hashes, first as usize, count as usize).unwrap()
                );
            }
            let plan = ExpandedPlan::derive(&f.class, f.artifact, &f.job, full_limits().plan).unwrap();
            let mut partial = false;
            let mut wide = false;
            for relation in plan.iter() {
                let values = adapter.expanded_claim(&plan, relation.index).unwrap();
                assert_eq!(values, oracle_claim(f, &relation));
                if let ExpandedRelationKind::RecomputeNode { dtype, lane_count, .. } = relation.kind {
                    partial |= lane_count < full_limits().plan.uncommitted_tile_lanes;
                    wide |= dtype == DType::I128 && values.iter().any(|v| v.unsigned_abs() > i64::MAX as u128);
                }
            }
            assert!(partial && wide);
            assert_eq!(runner.plan.program.encode(), f.encoded_program);
            assert_eq!(f.class.program, f.encoded_program);
        });
    }
}

#[test]
fn range_admissible_class_fixture_streams_actual_a1_full_relation_materials() {
    for window in [1, 3] {
        positive_typed_adapter_common::with_window_fixture(window, |f, runner, inventory| {
            let signed = SignedMiner::new(f);
            let authenticated = signed.authenticate();
            let original_result = signed.result.id();
            let original_signature = signed.signature;
            let capture = capture_typed_prototype(
                runner,
                &f.class,
                f.artifact,
                &f.job,
                f.job.context_hash(),
                positive_typed_adapter_common::PROMPT,
                capture_limits(),
            )
            .unwrap();
            let adapter = TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &authenticated,
                    rooted: &signed.rooted,
                    step_space: runner.space,
                },
                adapter_limits(),
            )
            .unwrap();
            let prepared = signed.preparation(f);
            let quote = quote_typed_adapter(&capture, &f.class, &f.job, prepared.plan(), stages(), adapter_limits()).unwrap();
            let claims = adapter.all_claims(prepared.plan(), stages()).unwrap();
            assert_eq!(claims.len() as u64, prepared.plan().summary().relations);
            for relation in prepared.plan().iter() {
                assert_eq!(claims[relation.index as usize], oracle_claim(f, &relation));
            }
            let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
            let record = prepared
                .freeze_record(
                    adapter.binding().unwrap(),
                    capture.run().generated.clone(),
                    principal(panel.verification_key.as_ref()),
                    claims,
                )
                .unwrap();
            let panel_signature =
                *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
                    .unwrap()
                    .as_ref();
            let mut checker = prepared.begin(record, panel.verification_key.as_ref(), &panel_signature).unwrap();
            let base = sources(inventory, positive_typed_adapter_common::PROMPT, &adapter.binding().unwrap(), capture.run());
            let total = checker.preparation().plan().summary().relations;
            assert!(total > 1);
            let mut tested_bad_path = false;
            let mut built_material_bytes = 0u64;
            let mut peak_material_bytes = 0u64;
            let mut peak_auth_storage_bytes = 0u64;
            while checker.progress().next_relation < total {
                let before = checker.progress();
                let relation = checker.preparation().plan().relation_at(before.next_relation).unwrap();
                if relation.index == 0 {
                    let meter = adapter.meter();
                    assert_eq!(
                        adapter
                            .material(
                                checker.preparation(),
                                relation.index,
                                before.legacy_frontier + 1,
                                &base,
                                &rules(),
                                full_limits().source
                            )
                            .err(),
                        Some(TypedAdapterError::Binding)
                    );
                    assert_eq!(checker.progress(), before);
                    assert_eq!(adapter.meter().material_bytes, meter.material_bytes);
                }
                let material = adapter
                    .material(checker.preparation(), relation.index, before.legacy_frontier, &base, &rules(), full_limits().source)
                    .unwrap();
                let charged = adapter.meter().material_bytes;
                assert_eq!(
                    adapter
                        .material(checker.preparation(), relation.index, before.legacy_frontier, &base, &rules(), full_limits().source)
                        .err(),
                    Some(TypedAdapterError::Budget)
                );
                assert_eq!(adapter.meter().material_bytes, charged);
                assert_eq!(checker.progress(), before);
                // This counts the actual borrowed material's structural bytes
                // and authentication storage, without another arithmetic run.
                let material_quote = material.as_borrowed().preflight(full_limits().source).unwrap();
                built_material_bytes = built_material_bytes.checked_add(material_quote.material_bytes).unwrap();
                peak_material_bytes = peak_material_bytes.max(material_quote.material_bytes);
                peak_auth_storage_bytes = peak_auth_storage_bytes.max(material_quote.logical_auth_storage_bytes);
                let legacy = legacy_for(&adapter, &relation, capture.run());
                if window == 1 && !tested_bad_path {
                    if let Some(valid) = legacy.as_ref() {
                        let mut corrupt = OwnedLegacy {
                            preimage: valid.preimage.clone(),
                            opening: valid.opening.clone(),
                            trace: valid.trace.clone(),
                        };
                        assert!(!corrupt.opening.siblings.is_empty());
                        corrupt.opening.siblings[0] = Hash64::from_u64_word(999);
                        assert_eq!(
                            checker.submit(&material.as_borrowed(), Some(corrupt.borrowed())),
                            Err(FullRelationError::Delivery)
                        );
                        assert_eq!(checker.progress().next_relation, before.next_relation);
                        assert_eq!(checker.progress().outcome, CheckOutcome::Checking);
                        assert!(checker.progress().charged_source_reads > before.charged_source_reads);
                        tested_bad_path = true;
                    }
                }
                checker
                    .submit(&material.as_borrowed(), legacy.as_ref().map(OwnedLegacy::borrowed))
                    .unwrap_or_else(|e| panic!("relation {relation:?}: {e:?}"));
                assert_eq!(checker.progress().next_relation, before.next_relation + 1);
                let spent = adapter.meter();
                drop(material);
                assert_eq!(adapter.meter(), spent); // Drop releases backing, never session work.
            }
            assert_eq!(checker.finish().unwrap(), CheckOutcome::Matched);
            assert_eq!(checker.progress().legacy_frontier, capture.run().leaf_count);
            assert_eq!(signed.result.id(), original_result);
            assert_eq!(signed.signature, original_signature);
            signed.authenticate(); // Original Miner signature remains literal and valid.
            assert_eq!(runner.plan.program.encode(), f.encoded_program);
            eprintln!(
                "typed-adapter window={window}: relations={total} capture={} index={} claims={} external_stage_envelope={} additional_peak={} total_live={} built_material_bytes={built_material_bytes} peak_material_bytes={peak_material_bytes} peak_logical_auth_storage_bytes={peak_auth_storage_bytes} meter={:?}",
                quote.capture_retained_bytes,
                quote.index_bytes,
                quote.all_claims_bytes,
                quote.external_stage_bytes,
                quote.additional_peak_bytes,
                quote.total_live_bytes,
                adapter.meter()
            );
        });
    }
}

#[test]
fn author_signed_altered_wide_claim_is_rejected_by_actual_source_recomputation() {
    positive_typed_adapter_common::with_fixture(|f, runner, inventory| {
        let signed = SignedMiner::new(f);
        let authenticated = signed.authenticate();
        let original_result = signed.result.id();
        let original_signature = signed.signature;
        let capture = capture_typed_prototype(
            runner,
            &f.class,
            f.artifact,
            &f.job,
            f.job.context_hash(),
            positive_typed_adapter_common::PROMPT,
            capture_limits(),
        )
        .unwrap();
        let adapter = TypedCaptureAdapter::prepare(
            &capture,
            TypedAdapterExpected {
                registered: &f.class,
                artifact: f.artifact,
                job: &f.job,
                result: &authenticated,
                rooted: &signed.rooted,
                step_space: runner.space,
            },
            adapter_limits(),
        )
        .unwrap();
        let prepared = signed.preparation(f);
        let mut claims = adapter.all_claims(prepared.plan(), stages()).unwrap();
        let bad = prepared
            .plan()
            .iter()
            .find(|r| {
                matches!(r.kind, ExpandedRelationKind::RecomputeNode { dtype: DType::I128, .. })
                    && claims[r.index as usize].iter().any(|v| v.unsigned_abs() > i64::MAX as u128)
            })
            .unwrap()
            .index;
        claims[bad as usize][0] ^= 1;
        let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
        let record = prepared
            .freeze_record(
                adapter.binding().unwrap(),
                capture.run().generated.clone(),
                principal(panel.verification_key.as_ref()),
                claims,
            )
            .unwrap();
        let signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
                .unwrap()
                .as_ref();
        let mut checker = prepared.begin(record, panel.verification_key.as_ref(), &signature).unwrap();
        let base = sources(inventory, positive_typed_adapter_common::PROMPT, &adapter.binding().unwrap(), capture.run());
        loop {
            let before = checker.progress();
            let relation = checker.preparation().plan().relation_at(before.next_relation).unwrap();
            let material = adapter
                .material(checker.preparation(), relation.index, before.legacy_frontier, &base, &rules(), full_limits().source)
                .unwrap();
            let legacy = legacy_for(&adapter, &relation, capture.run());
            let outcome = checker.submit(&material.as_borrowed(), legacy.as_ref().map(OwnedLegacy::borrowed));
            if relation.index == bad {
                assert_eq!(outcome, Err(FullRelationError::PanelMismatch));
                break;
            }
            outcome.unwrap();
        }
        assert_eq!(checker.progress().next_relation, bad);
        assert_eq!(checker.progress().outcome, CheckOutcome::WitnessRejected);
        assert_eq!(checker.finish(), Err(FullRelationError::Closed));
        assert_eq!(signed.result.id(), original_result);
        assert_eq!(signed.signature, original_signature);
        signed.authenticate();
    });
}

#[test]
fn independent_expected_identity_and_simultaneous_backing_caps_fail_closed() {
    positive_typed_capture_common::with_fixture(|f, runner, _| {
        let signed = SignedMiner::new(f);
        let authenticated = signed.authenticate();
        let capture = capture_typed_prototype(
            runner,
            &f.class,
            f.artifact,
            &f.job,
            f.job.context_hash(),
            positive_typed_capture_common::PROMPT,
            capture_limits(),
        )
        .unwrap();
        let plan = ExpandedPlan::derive(&f.class, f.artifact, &f.job, full_limits().plan).unwrap();
        let quote = quote_typed_adapter(&capture, &f.class, &f.job, &plan, stages(), adapter_limits()).unwrap();
        for field in 0..10 {
            let mut cap = adapter_limits();
            match field {
                0 => cap.max_index_records = capture.entries().len() as u64 - 1,
                1 => cap.max_index_bytes = quote.index_bytes - 1,
                2 => cap.max_claims_heap_bytes = quote.all_claims_bytes - 1,
                3 => cap.max_additional_heap_bytes = quote.additional_peak_bytes - 1,
                4 => cap.max_legacy_leaves = capture.run().leaf_count - 1,
                5 => cap.max_tile_lanes = plan.summary().peak_target_bytes / 16 - 1,
                6 => cap.max_path_siblings = 0,
                7 => cap.max_proof_leaf_reads = capture.run().leaf_count - 1,
                8 => cap.max_hash_work = 0,
                _ => cap.max_material_bytes = 0,
            }
            assert_eq!(quote_typed_adapter(&capture, &f.class, &f.job, &plan, stages(), cap), Err(TypedAdapterError::Budget));
        }
        let mut cap = adapter_limits();
        cap.max_index_records = 0;
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &authenticated,
                    rooted: &signed.rooted,
                    step_space: runner.space
                },
                cap
            )
            .err(),
            Some(TypedAdapterError::Budget)
        );
        let mut class = f.class.clone();
        class.tokenizer_id = Hash64::from_u64_word(999);
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &authenticated,
                    rooted: &signed.rooted,
                    step_space: runner.space
                },
                adapter_limits()
            )
            .err(),
            Some(TypedAdapterError::Binding)
        );
        // Two individually coherent identity chains are insufficient: the
        // foreign class declaration has equal program/layout and a re-signed Result,
        // while the ORIGINAL job/capture still name the original class.
        assert_eq!(class.program, f.class.program);
        assert_eq!(class.layout, f.class.layout);
        let mut disjoint_result = signed.result.clone();
        disjoint_result.registered_class = digest(class.class_id(&f.artifact));
        let mut disjoint_rooted = signed.rooted;
        disjoint_rooted.registered_class = disjoint_result.registered_class;
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
        let disjoint_signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &disjoint_result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32])
                .unwrap()
                .as_ref();
        let disjoint_auth = disjoint_result.authenticate(&signed.key, &disjoint_signature).unwrap();
        disjoint_rooted.check(ClaimForm::PositiveResultV2, &disjoint_result).unwrap();
        assert_eq!(disjoint_result.registered_class, digest(class.class_id(&f.artifact)));
        assert_eq!(capture.origin().class, f.job.shape_profile_id);
        assert_ne!(class.class_id(&f.artifact), capture.origin().class);
        assert_eq!(disjoint_result.tokenizer, digest(f.job.tokenizer_id));
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &disjoint_auth,
                    rooted: &disjoint_rooted,
                    step_space: runner.space
                },
                adapter_limits()
            )
            .err(),
            Some(TypedAdapterError::Binding)
        );
        let mut job = f.job.clone();
        job.execution_seed[0] ^= 1;
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: f.artifact,
                    job: &job,
                    result: &authenticated,
                    rooted: &signed.rooted,
                    step_space: runner.space
                },
                adapter_limits()
            )
            .err(),
            Some(TypedAdapterError::Binding)
        );
        let mut rooted = signed.rooted;
        rooted.expected_input = d(99);
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &authenticated,
                    rooted: &rooted,
                    step_space: runner.space
                },
                adapter_limits()
            )
            .err(),
            Some(TypedAdapterError::Binding)
        );
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: Hash64::from_u64_word(999),
                    job: &f.job,
                    result: &authenticated,
                    rooted: &signed.rooted,
                    step_space: runner.space
                },
                adapter_limits()
            )
            .err(),
            Some(TypedAdapterError::Binding)
        );
        let mut other_class = f.class.clone();
        other_class.layout.checkpoint_interval += 1;
        let other_space = PalwTirStepSpaceV1::new(&other_class).unwrap();
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &authenticated,
                    rooted: &signed.rooted,
                    step_space: &other_space
                },
                adapter_limits()
            )
            .err(),
            Some(TypedAdapterError::Binding)
        );
        let mut other_job = f.job.clone();
        other_job.job_id = Hash64::from_u64_word(998);
        let other_capture = capture_typed_prototype(
            runner,
            &f.class,
            f.artifact,
            &other_job,
            other_job.context_hash(),
            positive_typed_capture_common::PROMPT,
            capture_limits(),
        )
        .unwrap();
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &other_capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &authenticated,
                    rooted: &signed.rooted,
                    step_space: runner.space
                },
                adapter_limits()
            )
            .err(),
            Some(TypedAdapterError::Binding)
        );
        let mut foreign = signed.result.clone();
        foreign.execution = d(97);
        let miner = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
        let foreign_signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&miner.signing_key, &foreign.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        let foreign_auth = foreign.authenticate(&signed.key, &foreign_signature).unwrap();
        assert_eq!(
            TypedCaptureAdapter::prepare(
                &capture,
                TypedAdapterExpected {
                    registered: &f.class,
                    artifact: f.artifact,
                    job: &f.job,
                    result: &foreign_auth,
                    rooted: &signed.rooted,
                    step_space: runner.space
                },
                adapter_limits()
            )
            .err(),
            Some(TypedAdapterError::Binding)
        );
    });
}

#[test]
fn legacy_hash_path_corruption_and_failed_proof_attempts_do_not_reset_session_meter() {
    positive_typed_capture_common::with_fixture(|f, runner, inventory| {
        let signed = SignedMiner::new(f);
        let authenticated = signed.authenticate();
        let capture = capture_typed_prototype(
            runner,
            &f.class,
            f.artifact,
            &f.job,
            f.job.context_hash(),
            positive_typed_capture_common::PROMPT,
            capture_limits(),
        )
        .unwrap();
        let adapter = TypedCaptureAdapter::prepare(
            &capture,
            TypedAdapterExpected {
                registered: &f.class,
                artifact: f.artifact,
                job: &f.job,
                result: &authenticated,
                rooted: &signed.rooted,
                step_space: runner.space,
            },
            adapter_limits(),
        )
        .unwrap();
        let preimage = adapter.legacy_preimage(0).unwrap();
        let opening = adapter.legacy_opening(0).unwrap();
        let before = adapter.meter();
        let mut corrupt = preimage.clone();
        corrupt.values_le[0] ^= 1;
        assert_eq!(adapter.validate_legacy(0, &corrupt, &opening), Err(TypedAdapterError::Binding));
        let mut wrong_path = opening.clone();
        assert!(!wrong_path.siblings.is_empty());
        wrong_path.siblings[0] = Hash64::from_u64_word(999);
        assert_eq!(adapter.validate_legacy(0, &preimage, &wrong_path), Err(TypedAdapterError::Binding));
        assert_eq!(adapter.meter().proof_attempts, before.proof_attempts + 2);
        adapter.validate_legacy(0, &preimage, &opening).unwrap();
        assert_eq!(adapter.meter().proof_attempts, before.proof_attempts + 3);
        let mut cap = adapter_limits();
        cap.max_proof_attempts = 1;
        let limited = TypedCaptureAdapter::prepare(
            &capture,
            TypedAdapterExpected {
                registered: &f.class,
                artifact: f.artifact,
                job: &f.job,
                result: &authenticated,
                rooted: &signed.rooted,
                step_space: runner.space,
            },
            cap,
        )
        .unwrap();
        assert_eq!(limited.legacy_opening(u64::MAX), Err(TypedAdapterError::Geometry));
        assert_eq!(limited.meter().proof_attempts, 1);
        let binding = limited.binding().unwrap();
        let base = sources(inventory, positive_typed_capture_common::PROMPT, &binding, capture.run());
        let meter = limited.meter();
        limited.store(&base).begin_builder_move_v1();
        assert_eq!(limited.legacy_opening(0), Err(TypedAdapterError::Budget));
        assert_eq!(limited.legacy_range_siblings(0, 1), Err(TypedAdapterError::Budget));
        assert_eq!(limited.meter(), meter);
    });
}

#[test]
fn live_material_backing_is_reserved_during_claim_export_and_released_without_meter_reset() {
    positive_typed_adapter_common::with_fixture(|f, runner, inventory| {
        let signed = SignedMiner::new(f);
        let authenticated = signed.authenticate();
        let capture = capture_typed_prototype(
            runner,
            &f.class,
            f.artifact,
            &f.job,
            f.job.context_hash(),
            positive_typed_adapter_common::PROMPT,
            capture_limits(),
        )
        .unwrap();
        let prepared = signed.preparation(f);
        let first = prepared.plan().relation_at(0).unwrap();
        assert_eq!(first.legacy_frontier, 0);
        let ExpandedRelationKind::RecomputeNode { block, node, .. } = &first.kind else {
            panic!("first relation must be the original gather");
        };
        assert!(matches!(f.program.blocks[*block as usize].nodes[*node as usize].prim, Prim::Gather { .. }));
        let quote = quote_typed_adapter(&capture, &f.class, &f.job, prepared.plan(), stages(), adapter_limits()).unwrap();
        let source = full_limits().source;
        let runtime_scratch = source
            .max_material_bytes
            .checked_mul(2)
            .unwrap()
            .checked_add(source.max_auth_storage_bytes)
            .unwrap()
            .checked_add(source.max_workspace_bytes)
            .unwrap()
            .checked_add(source.max_target_bytes)
            .unwrap();
        let mut cap = adapter_limits();
        cap.max_additional_heap_bytes = quote
            .index_bytes
            .checked_add(quote.input_auth_scratch_bytes)
            .unwrap()
            .checked_add(quote.binding_clone_bytes)
            .unwrap()
            .checked_add(runtime_scratch)
            .unwrap()
            .checked_add(8)
            .unwrap();
        let adapter = TypedCaptureAdapter::prepare(
            &capture,
            TypedAdapterExpected {
                registered: &f.class,
                artifact: f.artifact,
                job: &f.job,
                result: &authenticated,
                rooted: &signed.rooted,
                step_space: runner.space,
            },
            cap,
        )
        .unwrap();
        assert_eq!(adapter.meter().claim_bytes, 0); // No all-claims allocation precedes this check.
        let base = sources(inventory, positive_typed_adapter_common::PROMPT, &adapter.binding().unwrap(), capture.run());
        let material = adapter.material(&prepared, first.index, 0, &base, &rules(), source).unwrap();
        assert!(material.as_borrowed().operands.preimages.is_empty());
        // This isolated export adds no external integration stages. Even the
        // outer claims Vec must reserve its headers against LIVE source scratch
        // before its allocation; no per-relation buffer is reached on refusal.
        let isolated_claim_stages = StageReservations {
            retained_preparation_bytes: 0,
            source_auth_bytes: 0,
            evaluator_bytes: 0,
            signature_workspace_bytes: 0,
        };
        let before_headers = adapter.meter();
        assert_eq!(adapter.all_claims(prepared.plan(), isolated_claim_stages), Err(TypedAdapterError::Budget));
        let after_headers = adapter.meter();
        assert_eq!(
            after_headers.claim_bytes - before_headers.claim_bytes,
            prepared.plan().summary().relations * std::mem::size_of::<Vec<i128>>() as u64
        );
        assert_eq!(after_headers.material_bytes, before_headers.material_bytes);
        let before = adapter.meter();
        assert_eq!(adapter.expanded_claim(prepared.plan(), first.index), Err(TypedAdapterError::Budget));
        let spent = adapter.meter();
        assert!(spent.claim_bytes > before.claim_bytes);
        assert_eq!(spent.material_bytes, before.material_bytes);
        drop(material);
        assert_eq!(adapter.meter(), spent); // Release backing, not cumulative reservations.
        assert_eq!(adapter.expanded_claim(prepared.plan(), first.index).unwrap(), oracle_claim(f, &first));
        assert!(adapter.meter().claim_bytes > spent.claim_bytes);
        assert_eq!(adapter.meter().material_bytes, spent.material_bytes);
    });
}

#[test]
fn original_sparse_declaration_without_static_range_guarantee_stays_unverified_at_source_authentication() {
    use kaspa_consensus_core::palw_tir_court_v1::source_only::{TirSourceOnlyErrorV1, authenticate_source_only_v1};
    positive_typed_capture_common::with_fixture(|f, runner, inventory| {
        // This is failure of a sufficient static range condition, not evidence
        // of overflow in this concrete run and not native class acceptance.
        assert!(misaka_palw_tir::interval::analyze_ranges(&f.program).is_err());
        let signed = SignedMiner::new(f);
        let authenticated = signed.authenticate();
        let original_result = signed.result.id();
        let original_signature = signed.signature;
        let capture = capture_typed_prototype(
            runner,
            &f.class,
            f.artifact,
            &f.job,
            f.job.context_hash(),
            positive_typed_capture_common::PROMPT,
            capture_limits(),
        )
        .unwrap();
        assert_eq!(capture.run().generated, EXPECTED);
        let adapter = TypedCaptureAdapter::prepare(
            &capture,
            TypedAdapterExpected {
                registered: &f.class,
                artifact: f.artifact,
                job: &f.job,
                result: &authenticated,
                rooted: &signed.rooted,
                step_space: runner.space,
            },
            adapter_limits(),
        )
        .unwrap();
        let prepared = signed.preparation(f);
        let claims = adapter.all_claims(prepared.plan(), stages()).unwrap();
        let binding = adapter.binding().unwrap();
        kaspa_consensus_core::palw_tir_step_v1::verify_tir_binding_v1(&binding, full_limits().plan.max_legacy_leaves).unwrap();
        let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
        let record = prepared
            .freeze_record(binding.clone(), capture.run().generated.clone(), principal(panel.verification_key.as_ref()), claims)
            .unwrap();
        let panel_signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
                .unwrap()
                .as_ref();
        let mut checker = prepared.begin(record, panel.verification_key.as_ref(), &panel_signature).unwrap();
        let before = checker.progress();
        assert_eq!(before.outcome, CheckOutcome::Checking);
        let base = sources(inventory, positive_typed_capture_common::PROMPT, &binding, capture.run());
        let material = adapter
            .material(checker.preparation(), before.next_relation, before.legacy_frontier, &base, &rules(), full_limits().source)
            .unwrap();
        // Builder output is unauthenticated material, not a valid-calculation
        // claim. The existing source-authentication range guard is preserved.
        assert!(matches!(
            authenticate_source_only_v1(&binding, material.as_borrowed(), before.legacy_frontier, &rules(), full_limits().source),
            Err(TirSourceOnlyErrorV1::Binding)
        ));
        assert_eq!(checker.submit(&material.as_borrowed(), None), Err(FullRelationError::Delivery));
        let after = checker.progress();
        assert_eq!(after.outcome, CheckOutcome::Checking);
        assert_eq!(after.next_relation, before.next_relation);
        assert_eq!(after.legacy_frontier, before.legacy_frontier);
        assert_eq!(after.checked_rows, before.checked_rows);
        assert!(after.charged_material_bytes > before.charged_material_bytes);
        assert!(after.charged_source_reads > before.charged_source_reads);
        assert_eq!(checker.finish(), Err(FullRelationError::Incomplete));
        assert_eq!(checker.progress().outcome, CheckOutcome::Checking);
        assert_eq!(signed.result.id(), original_result);
        assert_eq!(signed.signature, original_signature);
        signed.authenticate();
        assert_eq!(runner.plan.program.encode(), f.encoded_program);
        assert_eq!(f.class.program, f.encoded_program);
    });
}
