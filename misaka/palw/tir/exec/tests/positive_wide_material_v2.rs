#![cfg(feature = "positive-panel")]
//! Local programs; no native registration, public activation or credit assertion.
//! One production observed CPU run; the real Node independently recomputes.
mod run_wide_common;
use kaspa_consensus_core::{
    Hash64,
    palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOpeningV1},
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_positive_check_v2::{
        CheckOutcome,
        expanded_plan::{ExpandedPlanLimits, ExpandedRelationKind},
        full_relation::{FullRelationError, FullRelationLimits, PrototypeDomains, wide_v2::FullRelationPreparationV2},
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::source_only::TirSourceOnlyLimitsV1,
    palw_tir_step_v1::PalwTirStepSpaceV2,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{CONTRACT_REVISION, CONTRACT_VERSION, identity::*, messages::*, tir::greedy_selector_id};
use misaka_palw_tir_exec::node::{
    TirHeldInventoryV1, TirParamOpenerV1, TirParamsSourceV1,
    memory::TirMemoryReserverV1,
    positive_wide_material_v2::{
        CaptureExpectedV2, WidePanelBuilderV2, WidePanelDiskReserverV2, WidePanelLimitsV2, WidePanelOwnersV2, WidePanelWorkV2,
        WideProviderEnvelopeV2, capture_one_pass_v2,
    },
    run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2},
};
use run_wide_common::{EXPECTED_GENERATED, Fixture, LEAF_CAP, MAX_CONTEXT, PROMPT};
use std::sync::Arc;
const EXPECTED: &[u32] = EXPECTED_GENERATED;
trait OracleFixture {
    fn class(&self) -> &PalwTirClassV1;
    fn artifact(&self) -> Hash64;
    fn job(&self) -> &PalwJobContextV2;
}
impl OracleFixture for Fixture<'_> {
    fn class(&self) -> &PalwTirClassV1 {
        &self.class
    }
    fn artifact(&self) -> Hash64 {
        self.artifact
    }
    fn job(&self) -> &PalwJobContextV2 {
        &self.job
    }
}

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn digest(n: Hash64) -> Digest {
    Digest(n.as_bytes())
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
fn domains() -> PrototypeDomains {
    PrototypeDomains::local_only(b"A1/full-relation/local-test-record", b"A1/FULL/LOCAL/TEST").unwrap()
}
struct SignedMiner {
    rooted: AdmissionContext,
    result: MinerResultCommit,
    key: [u8; PUBKEY_BYTES],
    signature: [u8; SIGNATURE_BYTES],
}
impl SignedMiner {
    fn new(f: &impl OracleFixture, wrong: bool) -> Self {
        assert_eq!(f.class().tokenizer_id, Hash64::from_u64_word(18));
        let output = if wrong { d(99) } else { digest(palw_attempt_output_root_v1(f.job(), EXPECTED)) };
        Self::for_output(f, output)
    }
    fn for_output(f: &impl OracleFixture, output: Digest) -> Self {
        let job = f.job();
        assert_eq!(job.tokenizer_id, Hash64::from_u64_word(0));
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
        let key = *pair.verification_key.as_ref();
        let rooted = Self::context(f, &key);
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
            output,
            dependencies: d(21),
            service_contract: d(22),
        };
        let signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        Self { rooted, result, key, signature }
    }
    fn context(f: &impl OracleFixture, key: &[u8; PUBKEY_BYTES]) -> AdmissionContext {
        let class = f.class();
        let job = f.job();
        let artifact = f.artifact();
        AdmissionContext {
            scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
            expected_job: digest(job.job_id),
            expected_execution: digest(job.context_hash()),
            expected_miner: principal(key),
            expected_input: digest(job.prompt_token_ids_hash),
            registered_class: digest(class.class_id(&artifact)),
            artifact: digest(artifact),
            program: digest(class.graph_ir_root()),
            tokenizer: digest(job.tokenizer_id),
            execution_rules: digest(job.cu_ruleset_id),
            selector: greedy_selector_id(),
            required_form: ClaimForm::PositiveResultV2,
        }
    }
    fn authenticate(&self) -> AuthenticatedResult<'_> {
        self.result.authenticate(&self.key, &self.signature).unwrap()
    }
    fn preparation(&self, f: &impl OracleFixture) -> FullRelationPreparationV2 {
        FullRelationPreparationV2::authenticate(
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
fn reserve() -> TirMemoryReserverV1 {
    Arc::new(|_, _, _, bytes| {
        assert!(bytes > 0);
        Ok(Box::new(()) as Box<dyn Send + Sync>)
    })
}
fn disk_reserve() -> WidePanelDiskReserverV2 {
    Arc::new(|_, _, bytes| {
        assert!(bytes > 0);
        Ok(Box::new(()) as Box<dyn Send + Sync>)
    })
}
fn bounds() -> WidePanelLimitsV2 {
    WidePanelLimitsV2 {
        max_records: 4096,
        max_step_leaves: LEAF_CAP,
        max_tape_bytes: 1 << 26,
        max_live_heap_bytes: 1 << 35,
        max_visits: 1 << 42,
        max_io_bytes: 1 << 42,
        max_hash_work: 1 << 48,
        max_proof_attempts: 1 << 20,
        max_provider_bytes: 1 << 42,
        max_provider_scratch: 1 << 24,
        max_material_bytes: 1 << 20,
        max_claim_bytes: 1 << 20,
        max_trace_frames: 1 << 16,
        external_retained_bytes: 1 << 28,
    }
}
fn run_bounds() -> TirWideRunLimitsV2 {
    TirWideRunLimitsV2 {
        max_positions: MAX_CONTEXT,
        max_step_leaves: LEAF_CAP,
        max_row_lanes: 16,
        max_work: 1 << 40,
        max_requested_bytes: 1 << 30,
    }
}
fn owners() -> WidePanelOwnersV2 {
    WidePanelOwnersV2 {
        material_limits: TirSourceOnlyLimitsV1 {
            max_source_reads: 256,
            max_target_lanes: 1 << 16,
            max_target_bytes: 1 << 20,
            max_material_bytes: 1 << 20,
            max_material_units: 1 << 16,
            max_auth_storage_bytes: 1 << 24,
            max_workspace_bytes: 1 << 29,
        },
        metadata_auth_bytes: 1 << 29,
        metadata_auth_work: 1 << 24,
        source_auth_bytes: 1 << 24,
        signature_bytes: 1 << 20,
        retained_record_bytes: 1 << 26,
    }
}
fn provider() -> WideProviderEnvelopeV2 {
    WideProviderEnvelopeV2 { per_call_io: 1 << 20, per_call_work: 1 << 20, max_result_bytes: 1 << 16 }
}
// Local producer values only; this tuple is not a new peer/native codec.
fn material_encoding(material: &misaka_palw_tir_exec::node::positive_wide_material_v2::WidePanelMaterialV2<'_, '_, '_>) -> Vec<u8> {
    let m = material.as_borrowed();
    borsh::to_vec(&(
        m.operands,
        m.params,
        m.prompt_token_ids,
        m.prompt_ids_openings,
        m.decode_tokens.map(|p| (p.rows_root, &p.generated_token_ids)),
        material.target().map(|t| (t.opening, t.preimage, t.trace)),
    ))
    .unwrap()
}
fn material_meter_delta(
    after: misaka_palw_tir_exec::node::positive_wide_material_v2::WidePanelMeterV2,
    before: misaka_palw_tir_exec::node::positive_wide_material_v2::WidePanelMeterV2,
) -> misaka_palw_tir_exec::node::positive_wide_material_v2::WidePanelMeterV2 {
    use misaka_palw_tir_exec::node::positive_wide_material_v2::WidePanelMeterV2;
    WidePanelMeterV2 {
        visits: after.visits - before.visits,
        io_bytes: after.io_bytes - before.io_bytes,
        hash_work: after.hash_work - before.hash_work,
        proof_attempts: after.proof_attempts - before.proof_attempts,
        provider_bytes: after.provider_bytes - before.provider_bytes,
        copied_bytes: after.copied_bytes - before.copied_bytes,
        tape_bytes: after.tape_bytes - before.tape_bytes,
    }
}
fn exercise(f: &Fixture<'_>, wrong_miner: bool, wrong_panel: bool) {
    let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
    let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.class.class_id(&f.artifact)).unwrap();
    let reserver = reserve();
    let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
    let capture = capture_one_pass_v2(
        CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
        &runner,
        run_bounds(),
        &work,
        tempfile::tempfile().unwrap(),
        &reserver,
    )
    .unwrap();
    assert_eq!(capture.run().generated(), EXPECTED);
    let rows = capture.run().logits_rows();
    let vocab = rows[0].len();
    let logit_values = (rows.len() * vocab) as u64;
    let event = capture.public_logits_event(0, 0, logit_values).unwrap();
    event.authenticate(&f.job, &capture.run().trace_root(), vocab, capture.run().leaf_count()).unwrap();
    assert!(capture.public_logits_event(0, 0, logit_values - 1).is_err());
    let signed = SignedMiner::new(f, wrong_miner);
    let r = &signed.result;
    let mut original = misaka_palw_positive_contract::public_one_honest::OriginalCommit {
        scope: r.scope,
        job: r.job,
        execution: r.execution,
        miner: r.miner,
        registered_class: r.registered_class,
        artifact: r.artifact,
        program: r.program,
        prompt: d(1),
        input_tokens: r.input,
        tokenizer: r.tokenizer,
        execution_rules: r.execution_rules,
        generation_seed: r.generation_seed,
        initial_state: d(2),
        dependencies: r.dependencies,
        prefill: r.prefill,
        decode: r.decode,
        max_context: r.max_context,
        output: d(0),
        trace_root: d(0),
        manifest_root: d(0),
        service_contract: r.service_contract,
    };
    let mut other = original.clone();
    other.execution = d(77);
    assert!(capture.bind_public_original(&mut other).is_err());
    assert_eq!(other.trace_root, d(0));
    capture.bind_public_original(&mut original).unwrap();
    assert_eq!(original.trace_root, digest(capture.run().execution_root()));
    let mut trace_index = capture.public_trace_index();
    let trace_len = trace_index.step_leaf_count;
    trace_index.check(&original, trace_len).unwrap();
    assert_eq!(
        borsh::to_vec(&trace_index).unwrap().len(),
        kaspa_consensus_core::palw_tir_court_v1::PublicOriginalTraceIndexV1::WIRE_BYTES
    );
    assert!(trace_index.check(&original, trace_len - 1).is_err());
    trace_index.step_merkle_root = Hash64::from_bytes([77; 64]);
    assert!(trace_index.check(&original, trace_len).is_err());
    let (opening, tile) = capture.public_trace_tile(0, trace_len, u64::MAX).unwrap();
    kaspa_consensus_core::palw_tir_court_v1::check_public_original_step_data_v1(
        &original,
        &capture.public_step_root(),
        0,
        &opening,
        &tile,
        trace_len,
        tile.values_le.len(),
    )
    .unwrap();
    let trace_material = kaspa_consensus_core::palw_tir_court_v1::PublicOriginalTraceTileV1 {
        original: original.id(),
        trace: capture.public_trace_index(),
        opening,
        tile,
    };
    let cap = trace_material.tile.values_le.len();
    trace_material.authenticate(&original, 0, trace_len, cap).unwrap();
    assert!(matches!(
        trace_material.wide_geometry(&original, &f.job, &f.class, 0, trace_len, cap).unwrap(),
        kaspa_consensus_core::palw_tir_court_v1::PublicWideTileGeometryV1::Canonical { leaf } if leaf.index == 0
    ));
    let mut other_original = original.clone();
    other_original.manifest_root = d(99);
    assert!(trace_material.authenticate(&other_original, 0, trace_len, cap).is_err());
    assert!(capture.public_trace_tile(0, trace_len, 0).is_err());
    assert!(capture.public_trace_tile(trace_len, trace_len, u64::MAX).is_err());
    use kaspa_consensus_core::{
        palw_step_refute::PalwStepRefuteError,
        palw_tir_court_v1::{PalwTirCourtRulesV1, PublicOriginalTraceTileV1, check_public_original_wide_greedy_decode_v1},
    };
    let logits_leaf = space
        .leaves_of_position(&f.job, f.job.declared_prefill_tokens - 1)
        .into_iter()
        .find(|leaf| leaf.dtype == misaka_palw_tir::DType::I64)
        .unwrap();
    let (opening, tile) = capture.public_trace_tile(logits_leaf.index, trace_len, u64::MAX).unwrap();
    let material = PublicOriginalTraceTileV1 { original: original.id(), trace: capture.public_trace_index(), opening, tile };
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: trace_len,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: misaka_palw_tir::demand::DemandLimits::UNLIMITED,
    };
    let binding = capture.binding().unwrap();
    let selected = event.generated[0] as usize;
    assert!(matches!(
        check_public_original_wide_greedy_decode_v1(&original, &binding, &event, &event, selected, &rules),
        Err(PalwStepRefuteError::NoFaultFound)
    ));
    let mut other_output = original.clone();
    other_output.output = d(99);
    assert!(matches!(
        check_public_original_wide_greedy_decode_v1(&other_output, &binding, &event, &event, selected, &rules),
        Err(PalwStepRefuteError::InputSetNotCanonical("wide decode court output does not bind the original"))
    ));
    let mut other_job = original.clone();
    other_job.job = d(99);
    assert!(matches!(
        check_public_original_wide_greedy_decode_v1(&other_job, &binding, &event, &event, selected, &rules),
        Err(PalwStepRefuteError::InputSetNotCanonical("public court binding names another original execution"))
    ));
    assert!(matches!(
        material.check_wide_logits_consistency(&original, &binding, logits_leaf.index, &event, material.tile.values_le.len(), &rules,),
        Err(PalwStepRefuteError::NoFaultFound)
    ));
    use kaspa_consensus_core::palw_tir_court_v1::PublicOriginalTraceCountV1;
    use kaspa_consensus_core::{
        palw_step_leg::{PalwStepFaultV1, step_merkle_root_v1, step_opening_v1, step_tile_leaf_hash_v1},
        palw_tir_step_v1::palw_tir_execution_root_v1,
    };
    let mut bad_index = capture.public_trace_index();
    let count_evidence = PublicOriginalTraceCountV1 { original: original.id(), trace: bad_index.clone() };
    assert!(matches!(
        count_evidence.check_wide_leaf_count(&original, &f.job, &f.class, trace_len),
        Err(PalwStepRefuteError::NoFaultFound)
    ));
    for declared in [0, trace_len + 1] {
        bad_index.step_leaf_count = declared;
        let mut wrong_count = original.clone();
        wrong_count.trace_root = digest(palw_tir_execution_root_v1(
            &f.job.context_hash(),
            &bad_index.full_logits_trace_root,
            &f.job.shape_profile_id,
            bad_index.step_leaf_count,
            &bad_index.step_merkle_root,
        ));
        let count_evidence = PublicOriginalTraceCountV1 { original: wrong_count.id(), trace: bad_index.clone() };
        assert!(count_evidence.check_wide_leaf_count(&original, &f.job, &f.class, trace_len).is_err());
        let count_verdict = count_evidence.check_wide_leaf_count(&wrong_count, &f.job, &f.class, trace_len).unwrap();
        assert_eq!(count_verdict.fault(), &PalwStepFaultV1::StepLeafCountNotCanonical);
        assert_eq!(count_verdict.key().0, wrong_count.id());
    }
    // Commit a different step value while preserving the independently
    // committed logits event. The changed tile really opens the changed claim.
    let mut bad = material;
    bad.tile.values_le[0] ^= 1;
    let context = f.job.context_hash();
    let class_id = f.job.shape_profile_id;
    let mut hashes = capture.run().leaf_hashes().to_vec();
    hashes[logits_leaf.index as usize] = step_tile_leaf_hash_v1(&context, &class_id, &bad.tile);
    bad.opening = step_opening_v1(&hashes, logits_leaf.index).unwrap();
    bad.trace.step_merkle_root = step_merkle_root_v1(&hashes).unwrap();
    let mut bad_binding = capture.binding().unwrap();
    bad_binding.step_merkle_root = bad.trace.step_merkle_root;
    bad_binding.committed_execution_root =
        palw_tir_execution_root_v1(&context, &bad.trace.full_logits_trace_root, &class_id, trace_len, &bad.trace.step_merkle_root);
    let mut bad_original = original.clone();
    bad_original.manifest_root = d(4);
    bad_original.trace_root = digest(bad_binding.committed_execution_root);
    bad.original = bad_original.id();
    let verdict = bad
        .check_wide_logits_consistency(&bad_original, &bad_binding, logits_leaf.index, &event, bad.tile.values_le.len(), &rules)
        .unwrap();
    assert_eq!(verdict.fault(), &PalwStepFaultV1::TirLogitsTraceMismatch { value_index: 0 });
    assert_eq!(verdict.key().0, bad_original.id());
    assert_eq!(original.output, digest(palw_attempt_output_root_v1(&f.job, EXPECTED)));
    use misaka_palw_positive_contract::public_one_honest::{ManifestScopeRules, tir_output_material_codec_v1};
    let size = original.decode * 4;
    let rules = ManifestScopeRules { codec: tir_output_material_codec_v1(), max_bytes: u64::from(size), chunk_bytes: size };
    let output = capture.public_output_material(rules).unwrap();
    assert_eq!(output.content_root, original.output);
    assert_eq!(output.chunk_range(0), Ok(0..u64::from(size)));
    assert!(capture.public_output_material(ManifestScopeRules { chunk_bytes: size - 1, ..rules }).is_err());
    // The admission adapter authenticates the signed commitment without
    // fetching its manifest. This fixture tests that boundary only.
    use kaspa_consensus_core::palw_positive_contract_v2::PublicOriginalAdmissionV1;
    use misaka_palw_positive_contract::public_one_honest::{ManifestV1, ORIGINAL_SIGN_CONTEXT};
    assert!(capture.check_public_original_replay(&original, EXPECTED).is_err());
    original.manifest_root = d(4);
    capture.check_public_original_replay(&original, EXPECTED).unwrap();
    assert!(capture.check_public_original_replay(&original, &EXPECTED[1..]).is_err());
    let different_trace = misaka_palw_positive_contract::public_one_honest::OriginalCommit { trace_root: d(77), ..original.clone() };
    assert!(capture.check_public_original_replay(&different_trace, EXPECTED).is_err());
    let admission = PublicOriginalAdmissionV1 {
        scope: r.scope,
        job: &f.job,
        class: &f.class,
        artifact: f.artifact,
        miner: r.miner,
        prompt: d(1),
        initial_state: d(2),
        dependencies: r.dependencies,
        service_contract: r.service_contract,
        max_manifest_bytes: ManifestV1::WIRE_BYTES,
    };
    let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &original.id().0.0, ORIGINAL_SIGN_CONTEXT, [0; 32]).unwrap();
    assert_eq!(admission.authenticate(&original, &signed.key, signature.as_ref()), Ok(original.id()));
    use kaspa_consensus_core::palw_positive_contract_v2::PublicOriginalReservationV1;
    use misaka_palw_positive_contract::{
        public_one_honest::{FirstNoticeDemand, MaterialCaseDemand, ProtectedRescueDemand},
        reservations::RemainingCapacity,
        rows::AggregateUsage,
    };
    let body = u64::from(ManifestV1::WIRE_BYTES);
    let supply = RemainingCapacity { transmitted_bytes: body, additional_work: 1, retained_bytes: body };
    let budget = AggregateBudget {
        rounds: 1,
        unique_source_bytes: body,
        transmitted_bytes: body * 3,
        additional_work: 3,
        retained_bytes: body * 3,
        retention_until: 10_100,
        fee: 1,
        witness_attempts: 1,
    };
    let mut reservation = PublicOriginalReservationV1 {
        inclusion_daa: 100,
        notice: FirstNoticeDemand {
            draw_ready_by: 101,
            material_ready_by: 101,
            cold_acquisition_daa: 1,
            full_inspection_daa: 1,
            notice_inclusion_daa: 1,
            missing_material_detected_by: 101,
            material_request_inclusion_daa: 1,
        },
        case: MaterialCaseDemand {
            material_delivery_daa: 1,
            full_inspection_daa: 1,
            localization_daa: 1,
            filing_daa: 1,
            adjudication_and_recovery_daa: 1,
        },
        initial_supply: supply,
        rescue: ProtectedRescueDemand {
            max_drawn_bonds: 2,
            per_drawn_bond: supply,
            max_outsiders: 0,
            per_outsider: RemainingCapacity { transmitted_bytes: 0, additional_work: 0, retained_bytes: 0 },
            defensive: RemainingCapacity { transmitted_bytes: 0, additional_work: 0, retained_bytes: 0 },
        },
        usage: &AggregateUsage::default(),
        budget: &budget,
    };
    use misaka_palw_positive_contract::public_one_honest::SignedPublicOriginalV1;
    let envelope = SignedPublicOriginalV1 { original: original.clone(), key: signed.key, signature: *signature.as_ref() };
    let bytes = borsh::to_vec(&envelope).unwrap();
    assert_eq!(bytes.len(), SignedPublicOriginalV1::WIRE_BYTES);
    assert!(SignedPublicOriginalV1::decode(&bytes[..bytes.len() - 1]).is_err());
    let prepared = admission.prepare_signed(&SignedPublicOriginalV1::decode(&bytes).unwrap(), &reservation).unwrap();
    let reserved = d(21);
    let record = prepared.record(reserved);
    let terminal = prepared.schedule.terminal_by();
    assert_eq!(
        record.court_verdict_key(&prepared, reserved, terminal, &verdict),
        Err(misaka_palw_positive_contract::ContractError::Scope)
    );
    let bad_signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &bad_original.id().0.0, ORIGINAL_SIGN_CONTEXT, [0; 32]).unwrap();
    let bad_prepared = admission.prepare(&bad_original, &signed.key, bad_signature.as_ref(), &reservation).unwrap();
    let bad_record = bad_prepared.record(reserved);
    assert_eq!(bad_record.court_verdict_key(&bad_prepared, reserved, terminal, &verdict), Ok(verdict.key()));
    let verdict_record = bad_record.court_record(&bad_prepared, reserved, terminal, &verdict).unwrap();
    let restored = borsh::from_slice::<kaspa_consensus_core::palw_positive_contract_v2::PublicOriginalCourtRecordV1>(
        &borsh::to_vec(&verdict_record).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.check_restore(&bad_record, &bad_prepared, reserved, terminal, &verdict), Ok(()));
    assert_eq!(
        restored.check_restore(&bad_record, &bad_prepared, reserved, terminal - 1, &verdict),
        Err(misaka_palw_positive_contract::ContractError::Scope)
    );
    assert_eq!(
        restored.check_restore(&bad_record, &bad_prepared, d(22), terminal, &verdict),
        Err(misaka_palw_positive_contract::ContractError::Scope)
    );
    assert_eq!(
        bad_record.court_verdict_key(&bad_prepared, reserved, terminal + 1, &verdict),
        Err(misaka_palw_positive_contract::ContractError::Range)
    );
    assert_eq!(prepared.capacity.transmitted_bytes, body * 3);
    use misaka_palw_positive_contract::public_one_honest::{
        FULL_REPLAY_SIGN_CONTEXT, FullReplayApproval, PanelBond, PublicDrawMemberV1,
    };
    let bond = PanelBond { transaction: d(10), index: 1 };
    let second_bond = PanelBond { transaction: d(11), index: 1 };
    let members = [
        PublicDrawMemberV1 { bond, principal: r.miner, rescue_reservation: d(8) },
        PublicDrawMemberV1 { bond: second_bond, principal: r.miner, rescue_reservation: d(9) },
    ];
    let draw = prepared.prepare_draw(d(11), 101, &members, d(12)).unwrap();
    let record = borsh::to_vec(&draw.record()).unwrap();
    let restored: kaspa_consensus_core::palw_positive_contract_v2::PublicDrawRecordV1 = borsh::from_slice(&record).unwrap();
    assert_eq!(restored.check_restore(&draw), Ok(()));
    let reordered = prepared.prepare_draw(d(11), 101, &[members[1], members[0]], d(12)).unwrap();
    assert_eq!(restored.check_restore(&reordered), Err(misaka_palw_positive_contract::ContractError::Scope));
    let approval = draw.approval_context(bond).unwrap();
    assert_eq!(approval.original, original.id());
    assert_eq!(approval.member, r.miner);
    assert_eq!(approval.full_scope, original.id().full_replay_scope(d(12)));
    let other_plan = prepared.prepare_draw(d(11), 101, &members, d(13)).unwrap();
    assert_ne!(approval.full_scope, other_plan.approval_context(bond).unwrap().full_scope);
    assert_eq!(draw.rescue_capacity(bond), Ok(supply));
    use misaka_palw_positive_contract::public_one_honest::{
        Claimant, MANIFEST_REQUEST_SIGN_CONTEXT, ManifestRequest, ManifestRequestContext,
    };
    let mut request_context = ManifestRequestContext {
        original: &original,
        claimant: Claimant::DrawnBond(bond),
        signer: r.miner,
        inspection_scope: d(12),
        supply_obligation: d(13),
        reservation: d(8),
        max_manifest_bytes: ManifestV1::WIRE_BYTES,
        manifest_body_bytes_reserved: body,
        schedule: prepared.schedule,
        inclusion_daa: 101,
    };
    let request = ManifestRequest {
        original: original.id(),
        manifest_root: original.manifest_root,
        claimant: request_context.claimant,
        signer: request_context.signer,
        inspection_scope: request_context.inspection_scope,
        supply_obligation: request_context.supply_obligation,
        collect_until: prepared.schedule.filed_by(),
        reservation: request_context.reservation,
    };
    let request_sig =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [0; 32])
            .unwrap();
    let accepted =
        prepared.verify_drawn_manifest_request(&draw, &request, &request_context, &signed.key, request_sig.as_ref()).unwrap();
    assert_eq!(accepted.reservation(), request_context.reservation);
    assert_eq!(
        prepared.verify_drawn_manifest_request(&other_plan, &request, &request_context, &signed.key, request_sig.as_ref()),
        Err(misaka_palw_positive_contract::ContractError::Scope)
    );
    request_context.reservation = d(9);
    assert_eq!(
        prepared.verify_drawn_manifest_request(&draw, &request, &request_context, &signed.key, request_sig.as_ref()),
        Err(misaka_palw_positive_contract::ContractError::Budget)
    );
    request_context.reservation = d(8);
    request_context.manifest_body_bytes_reserved += 1;
    assert_eq!(
        prepared.verify_drawn_manifest_request(&draw, &request, &request_context, &signed.key, request_sig.as_ref()),
        Err(misaka_palw_positive_contract::ContractError::Budget)
    );
    request_context.manifest_body_bytes_reserved = body;
    request_context.claimant = Claimant::Outsider(r.miner);
    let outside = ManifestRequest { claimant: request_context.claimant, ..request };
    let outside_sig =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &outside.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [0; 32])
            .unwrap();
    assert_eq!(
        prepared.verify_outsider_manifest_request(0, &outside, &request_context, &signed.key, outside_sig.as_ref()),
        Err(misaka_palw_positive_contract::ContractError::Budget)
    );
    let outsider_budget = AggregateBudget { transmitted_bytes: body * 4, retained_bytes: body * 4, additional_work: 4, ..budget };
    let outsider_reservation = PublicOriginalReservationV1 {
        rescue: ProtectedRescueDemand { max_outsiders: 1, per_outsider: supply, ..reservation.rescue },
        budget: &outsider_budget,
        ..reservation
    };
    let with_outsider = admission.prepare(&original, &signed.key, signature.as_ref(), &outsider_reservation).unwrap();
    assert!(with_outsider.verify_outsider_manifest_request(0, &outside, &request_context, &signed.key, outside_sig.as_ref()).is_ok());
    assert!(with_outsider.verify_outsider_manifest_request(1, &outside, &request_context, &signed.key, outside_sig.as_ref()).is_err());
    assert!(draw.rescue_capacity(PanelBond { index: 2, ..bond }).is_err());
    assert!(prepared.prepare_draw(d(11), 100, &members, d(12)).is_err());
    assert!(prepared.prepare_draw(d(11), 101, &[members[0], members[0]], d(12)).is_err());
    let assertion = FullReplayApproval {
        original: approval.original,
        draw: approval.draw,
        bond,
        member: approval.member,
        full_scope: approval.full_scope,
    };
    let signed_approval =
        libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &assertion.signing_digest().0, FULL_REPLAY_SIGN_CONTEXT, [0; 32]).unwrap();
    let signed_assertion = (102, &assertion, &signed.key, signed_approval.as_ref());
    let approval_records = draw.verify_replay_quorum(1, 102, &[signed_assertion]).unwrap();
    assert_eq!(approval_records.len(), 1);
    let restored_approvals = borsh::from_slice::<Vec<kaspa_consensus_core::palw_positive_contract_v2::PublicReplayApprovalRecordV1>>(
        &borsh::to_vec(&approval_records).unwrap(),
    )
    .unwrap();
    assert_eq!(restored_approvals[0].check_restore(&draw, 102, 102, &assertion, &signed.key, signed_approval.as_ref()), Ok(()));
    assert_eq!(
        restored_approvals[0].check_restore(&draw, 102, 101, &assertion, &signed.key, signed_approval.as_ref()),
        Err(misaka_palw_positive_contract::ContractError::Scope)
    );
    assert_eq!(draw.verify_replay_quorum(1, prepared.schedule.hard(), &[signed_assertion]), Ok(restored_approvals));
    assert_ne!(
        draw.verify_replay_quorum(1, 102, &[(101, &assertion, &signed.key, signed_approval.as_ref())]).unwrap(),
        approval_records
    );
    assert_eq!(
        draw.verify_replay_quorum(1, prepared.schedule.hard() + 1, &[signed_assertion]),
        Err(misaka_palw_positive_contract::ContractError::Range)
    );
    assert!(draw.verify_replay_quorum(2, 102, &[signed_assertion]).is_err());
    assert_eq!(draw.verify_replay_quorum(1, 101, &[signed_assertion]), Err(misaka_palw_positive_contract::ContractError::Range));
    assert_eq!(
        draw.verify_replay_quorum(1, 102, &[(100, &assertion, &signed.key, signed_approval.as_ref())]),
        Err(misaka_palw_positive_contract::ContractError::Range)
    );
    assert_eq!(
        draw.verify_replay_quorum(2, 102, &[signed_assertion, signed_assertion]),
        Err(misaka_palw_positive_contract::ContractError::Identity)
    );
    let record = prepared.record(d(8));
    let restored = borsh::from_slice::<kaspa_consensus_core::palw_positive_contract_v2::PublicOriginalAdmissionRecordV1>(
        &borsh::to_vec(&record).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.check_restore(&prepared, d(8)), Ok(()));
    assert_eq!(restored.check_restore(&prepared, d(9)), Err(misaka_palw_positive_contract::ContractError::Scope));
    reservation.initial_supply.additional_work += 2;
    reservation.rescue.per_drawn_bond.additional_work -= 1;
    let shifted = admission.prepare(&original, &signed.key, signature.as_ref(), &reservation).unwrap();
    assert_eq!(shifted.capacity, prepared.capacity);
    assert_eq!(restored.check_restore(&shifted, d(8)), Err(misaka_palw_positive_contract::ContractError::Scope));
    let short = AggregateBudget { transmitted_bytes: body * 3 - 1, ..budget };
    reservation.budget = &short;
    assert_eq!(
        admission.prepare(&original, &signed.key, signature.as_ref(), &reservation),
        Err(misaka_palw_positive_contract::ContractError::Budget)
    );
    let mut wrong_service = original.clone();
    wrong_service.service_contract = d(5);
    assert_eq!(
        admission.authenticate(&wrong_service, &signed.key, signature.as_ref()),
        Err(misaka_palw_positive_contract::ContractError::Scope)
    );
    let result = signed.authenticate();
    let prep = signed.preparation(f);
    let source = TirParamsSourceV1(f.params);
    let inventory = TirHeldInventoryV1::build(f.program, &source).unwrap();
    assert_eq!(inventory.tree.root(), f.artifact);
    let mut builder =
        WidePanelBuilderV2::prepare(&capture, &prep, &result, &signed.rooted, &inventory, provider(), owners(), &reserver).unwrap();
    let tape = builder.claims_tape(tempfile::tempfile().unwrap()).unwrap();
    let mut claims = vec![];
    tape.visit(&mut |index, values| {
        assert_eq!(index as usize, claims.len());
        claims.push(values.to_vec());
        Ok(())
    })
    .unwrap();
    let mut encoded = vec![];
    tape.stream_borsh(&mut encoded).unwrap();
    assert_eq!(encoded, borsh::to_vec(&claims).unwrap());
    let n = claims.len();
    if wrong_panel {
        let i = prep.plan().iter().find(|r| matches!(r.kind, ExpandedRelationKind::RecomputeNode { .. })).unwrap().index as usize;
        claims[i][0] += 1;
    }
    let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    let observed_record = prep
        .freeze_record(
            capture.binding().unwrap(),
            capture.run().generated().to_vec(),
            principal(panel.verification_key.as_ref()),
            claims.clone(),
        )
        .unwrap();
    let record = prep
        .freeze_record(
            capture.binding().unwrap(),
            capture.run().generated().to_vec(),
            principal(panel.verification_key.as_ref()),
            claims,
        )
        .unwrap();
    let sig = *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
        .unwrap()
        .as_ref();
    // This separately authenticated Node preparation owns all numerical checking.
    assert_eq!(record.signing_digest(), observed_record.signing_digest());
    let mut observed = signed.preparation(f).begin(observed_record, panel.verification_key.as_ref(), &sig).unwrap();
    let mut checker = signed.preparation(f).begin(record, panel.verification_key.as_ref(), &sig).unwrap();
    let before = builder.meter();
    let mut rejected = false;
    let step_root = capture.public_step_root();
    let wrong_step_root = kaspa_consensus_core::palw_tir_court_v1::PublicOriginalStepRootV1 {
        full_logits_trace_root: Hash64::from_u64_word(1),
        step_leaf_count: step_root.step_leaf_count,
    };
    for index in 0..n {
        let progress = checker.progress();
        let before_first = builder.meter();
        let material = builder.material(index as u64, progress.legacy_frontier).unwrap();
        if let Some(target) = material.target() {
            use kaspa_consensus_core::palw_tir_court_v1::check_public_original_step_data_v1;
            let index = target.opening.leaf_index;
            let max_bytes = target.preimage.values_le.len();
            check_public_original_step_data_v1(
                &original,
                &step_root,
                index,
                target.opening,
                target.preimage,
                step_root.step_leaf_count,
                max_bytes,
            )
            .unwrap();
            assert!(
                check_public_original_step_data_v1(
                    &original,
                    &wrong_step_root,
                    index,
                    target.opening,
                    target.preimage,
                    step_root.step_leaf_count,
                    max_bytes,
                )
                .is_err()
            );
            assert!(
                check_public_original_step_data_v1(
                    &original,
                    &step_root,
                    index + 1,
                    target.opening,
                    target.preimage,
                    step_root.step_leaf_count,
                    max_bytes,
                )
                .is_err()
            );
        }
        assert!(builder.material(index as u64, progress.legacy_frontier).is_err());
        // The existing source/frame owners remain live; additionally own both
        // serialized test exports before allocation. Reused scratch and a
        // fresh builder/seed must both preserve canonical bytes and spent work.
        let _exports = reserver("test-repeated-material-exports", f.job.shape_profile_id, f.job.context_hash(), 4 << 20).unwrap();
        let first_bytes = material_encoding(&material);
        assert!(first_bytes.len() <= 2 << 20);
        let after_first = builder.meter();
        let outcome = checker.submit(&material.as_borrowed(), material.target());
        let mut diagnostics = kaspa_consensus_core::palw_tir_court_v1::source_only::wide_v2::EvaluationDiagnosticsV2::default();
        let observed_outcome = observed.submit_with_diagnostics(&material.as_borrowed(), material.target(), &mut diagnostics);
        assert_eq!(observed_outcome, outcome);
        assert_eq!(observed.progress(), checker.progress(), "observation must not change coverage, verdict or spent charges");
        assert_eq!(diagnostics.stage, kaspa_consensus_core::palw_tir_court_v1::source_only::wide_v2::EvaluationStageV2::Complete);
        assert!(diagnostics.work.is_some() && diagnostics.workspace_bytes.is_some());
        drop(material);
        let repeated = builder.material(index as u64, progress.legacy_frontier).unwrap();
        let second_bytes = material_encoding(&repeated);
        assert_eq!(first_bytes, second_bytes, "cleared scratch cannot change canonical materials");
        assert_eq!(
            material_meter_delta(after_first, before_first),
            material_meter_delta(builder.meter(), after_first),
            "scratch reuse cannot omit or refund cumulative work"
        );
        drop(repeated);
        drop(second_bytes);
        drop(builder);
        builder = WidePanelBuilderV2::prepare(&capture, &prep, &result, &signed.rooted, &inventory, provider(), owners(), &reserver)
            .unwrap();
        let before_fresh = builder.meter();
        let fresh = builder.material(index as u64, progress.legacy_frontier).unwrap();
        assert_eq!(first_bytes, material_encoding(&fresh), "fresh membership seed cannot change canonical materials");
        assert_eq!(
            material_meter_delta(after_first, before_first),
            material_meter_delta(builder.meter(), before_fresh),
            "fresh capacity/seed cannot omit or refund cumulative work"
        );
        drop(fresh);
        if outcome == Err(FullRelationError::PanelMismatch) {
            assert!(wrong_panel);
            assert_eq!(checker.progress().outcome, CheckOutcome::WitnessRejected);
            rejected = true;
            break;
        }
        outcome.unwrap();
        assert_eq!(checker.progress().next_relation, index as u64 + 1);
    }
    assert!(builder.meter().proof_attempts > before.proof_attempts);
    if wrong_panel {
        assert!(rejected);
    } else {
        let expected = if wrong_miner { CheckOutcome::OutputMismatch } else { CheckOutcome::Matched };
        assert_eq!(observed.finish().unwrap(), expected);
        assert_eq!(checker.finish().unwrap(), expected);
    }
    signed.authenticate();
}
#[test]
fn one_pass_wide_tape_materials_reach_real_node_for_windows_one_and_three() {
    for window in [1, 3] {
        run_wide_common::with_fixture(window, |f, _| exercise(f, false, false));
    }
}
#[test]
fn wrong_panel_claim_remains_witness_rejection_and_wrong_miner_waits_for_all_relations() {
    run_wide_common::with_fixture(3, |f, _| {
        exercise(f, false, true);
        exercise(f, true, false);
    });
}
struct Unknown<'a>(&'a dyn TirParamOpenerV1);
impl TirParamOpenerV1 for Unknown<'_> {
    fn param_opening(&self, i: u32) -> Option<PalwArtifactOpeningV1> {
        self.0.param_opening(i)
    }
    fn param_multiproof(&self, i: &[u32]) -> Option<PalwArtifactMultiproofV1> {
        self.0.param_multiproof(i)
    }
}
#[test]
fn unknown_provider_scratch_is_refused_without_resetting_session_costs() {
    run_wide_common::with_fixture(1, |f, _| {
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.class.class_id(&f.artifact)).unwrap();
        let reserve = reserve();
        let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
        let capture = capture_one_pass_v2(
            CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
            &runner,
            run_bounds(),
            &work,
            tempfile::tempfile().unwrap(),
            &reserve,
        )
        .unwrap();
        let s = SignedMiner::new(f, false);
        let auth = s.authenticate();
        let prep = s.preparation(f);
        let src = TirParamsSourceV1(f.params);
        let inv = TirHeldInventoryV1::build(f.program, &src).unwrap();
        let unknown = Unknown(&inv);
        let builder =
            WidePanelBuilderV2::prepare(&capture, &prep, &auth, &s.rooted, &unknown, provider(), owners(), &reserve).unwrap();
        let before = builder.meter();
        for _ in 0..2 {
            let err = match builder.material(0, 0) {
                Ok(_) => panic!("unknown provider accepted"),
                Err(e) => e,
            };
            assert!(err.contains("scratch unknown"));
        }
        let after = builder.meter();
        assert!(after.visits > before.visits);
        assert!(after.copied_bytes > before.copied_bytes);
        assert!(after.provider_bytes > before.provider_bytes);
    });
}
#[test]
fn failed_partial_trace_is_cleared_before_retry_without_refunding_work() {
    run_wide_common::with_fixture(1, |f, _| {
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
        let reserver = reserve();
        let mut l = bounds();
        l.max_trace_frames = 1;
        let work = WidePanelWorkV2::new(l, f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
        let capture = capture_one_pass_v2(
            CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
            &runner,
            run_bounds(),
            &work,
            tempfile::tempfile().unwrap(),
            &reserver,
        )
        .unwrap();
        let signed = SignedMiner::new(f, false);
        let result = signed.authenticate();
        let prep = signed.preparation(f);
        let source = TirParamsSourceV1(f.params);
        let inventory = TirHeldInventoryV1::build(f.program, &source).unwrap();
        let builder =
            WidePanelBuilderV2::prepare(&capture, &prep, &result, &signed.rooted, &inventory, provider(), owners(), &reserver)
                .unwrap();
        let attempt = || match builder.material(0, 0) {
            Ok(_) => panic!("one-frame trace unexpectedly completed"),
            Err(e) => e,
        };
        let before = builder.meter();
        assert!(attempt().contains("frame cap"));
        let after_first = builder.meter();
        assert!(attempt().contains("frame cap"));
        let delta = material_meter_delta(after_first, before);
        assert!(delta.visits >= 256 && delta.copied_bytes > 0);
        assert_eq!(delta, material_meter_delta(builder.meter(), after_first));
    });
}
#[test]
fn captured_gather_select_topk_and_fixed_history_feed_actual_node_without_discovery_evaluation() {
    use kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1;
    use misaka_palw_tir::{Cmp, DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder};
    use misaka_palw_tir_exec::{TirParams, TirPlan};
    // A NEW local declaration. Its flags/dtypes/layout are not a rewrite of a
    // registered model; no native registration receipt is asserted by this test.
    let mut b = ProgramBuilder::new(4, 1 << 18);
    let embed = b.param("e", DType::I8, &[4, 4], false);
    let logits_table = b.param("l", DType::I64, &[4, 4], false);
    let fixed = b.fixed_state("s", DType::I8, &[4], -128, 127, false);
    let hist = b.hist_state("h", DType::I8, &[4], 3, false);
    let zero = b.scalar(DType::Idx, 0);
    let pre = {
        let mut p = b.block("pre", vec![]);
        let row = p.gather(embed, Ref::Input(0), 0, 0);
        let top = p.topk(row, 0, 1);
        let top = p.reshape_fixed(top, &[]);
        let other = p.gather(embed, top, 0, 0);
        let cond = p.compare(Ref::Input(0), zero, Cmp::Eq);
        let chosen = p.select(cond, row, other, DType::I8);
        let acc = p.add(Ref::State(fixed), chosen, DType::I32);
        let bounded = p.clamp(acc, -128, 127, DType::I8);
        p.state_write(fixed, bounded);
        let rows = p.hist_append(hist, bounded);
        let sum = p.reduce_sum(rows, 0, DType::I32);
        let sum = p.reshape_fixed(sum, &[4]);
        let carry = p.clamp(sum, -128, 127, DType::I8);
        p.finish(&[carry])
    };
    let (post, logits) = {
        let mut p = b.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
        let last = p.gather(logits_table, Ref::Input(0), 0, 0);
        p.commit(last);
        let Ref::Node(n) = last else { unreachable!() };
        (p.finish(&[]), n)
    };
    let mut program = b.finish(pre, vec![], post, logits);
    program.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let mut map = MapParams::default();
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], run_wide_common::EMBEDDING.to_vec()).unwrap());
    map.tensors.insert((1, None), Tensor::new(DType::I64, vec![4, 4], run_wide_common::logits_table(true)).unwrap());
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let committed = program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 16,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: vec![4; committed],
            state_tiles: vec![4; 2],
        },
        tokenizer_id: Hash64::from_u64_word(18),
    };
    run_wide_common::with_fixture(3, |old, _| {
        let job = PalwJobContextV2 { shape_profile_id: class.class_id(&artifact), ..old.job.clone() };
        let f = Fixture {
            program: &program,
            plan: &plan,
            params: &params,
            class,
            artifact,
            job,
            nodes: vec![],
            steps: vec![],
            rows: vec![],
        };
        exercise(&f, false, false);
    });
}
#[test]
fn failed_capture_keeps_shared_spent_work_and_cannot_start_a_second_cpu_run() {
    use std::io::Write;
    run_wide_common::with_fixture(1, |f, _| {
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
        let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
        let shared = work.clone();
        let mut nonempty = tempfile::tempfile().unwrap();
        nonempty.write_all(b"not an empty tape").unwrap();
        let attempt = |file, work: &WidePanelWorkV2| {
            capture_one_pass_v2(
                CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
                &runner,
                run_bounds(),
                work,
                file,
                &reserve(),
            )
        };
        let first = match attempt(nonempty, &work) {
            Ok(_) => panic!("nonempty tape accepted"),
            Err(e) => e,
        };
        assert!(first.contains("empty"));
        let spent = shared.meter();
        assert!(spent.hash_work > 0);
        let second = match attempt(tempfile::tempfile().unwrap(), &shared) {
            Ok(_) => panic!("failed attempt reset"),
            Err(e) => e,
        };
        assert!(second.contains("already started"));
        assert_eq!(shared.meter(), spent);
    });
}
#[test]
fn oversized_typed_network_is_dropped_before_hashing_or_starting_the_cpu_run() {
    run_wide_common::with_fixture(1, |f, _| {
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
        // The work constructor receives only fixed IDs, never a delivered job.
        let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
        let mut delivered = f.job.clone();
        delivered.network_id = vec![0; kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES + 1];
        let before = work.meter();
        let error = match capture_one_pass_v2(
            CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &delivered, space: &space, prompt: PROMPT },
            &runner,
            run_bounds(),
            &work,
            tempfile::tempfile().unwrap(),
            &reserve(),
        ) {
            Ok(_) => panic!("oversized typed network accepted"),
            Err(e) => e,
        };
        assert!(error.contains("shape/length"));
        assert_eq!(work.meter(), before);
    });
}
#[test]
fn required_same_ledger_capture_and_run_guards_survive_until_owned_capture_drop() {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    struct Guard(Arc<AtomicUsize>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    run_wide_common::with_fixture(1, |f, _| {
        let live = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(Mutex::new(vec![]));
        let (counter, requests) = (live.clone(), calls.clone());
        let expected_class = f.job.shape_profile_id;
        let expected_execution = f.job.context_hash();
        let ledger: TirMemoryReserverV1 = Arc::new(move |role, class, execution, bytes| {
            assert_eq!((class, execution), (expected_class, expected_execution));
            assert!(bytes > 0);
            requests.lock().unwrap().push((role, bytes));
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Guard(counter.clone())) as Box<dyn Send + Sync>)
        });
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, expected_class).unwrap();
        let work = WidePanelWorkV2::new(bounds(), expected_class, expected_execution, &disk_reserve()).unwrap();
        let capture = capture_one_pass_v2(
            CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
            &runner,
            run_bounds(),
            &work,
            tempfile::tempfile().unwrap(),
            &ledger,
        )
        .unwrap();
        let requests = calls.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].0, "positive-wide-panel-capture-v2");
        assert_eq!(requests[1].0, "tir-wide-q24-v2-required");
        assert_eq!(requests[1].1, runner.quote(&f.job, run_bounds()).unwrap().requested_bytes);
        assert_eq!(requests[2].0, "tir-wide-q24-v2-retained");
        assert_eq!(requests[2].1, capture.run().reserved_bytes());
        assert_eq!(requests[2].1, runner.quote(&f.job, run_bounds()).unwrap().retained_output_requests().unwrap());
        drop(requests);
        assert_eq!(live.load(Ordering::SeqCst), 2);
        drop(capture);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    });
}
#[test]
fn denied_capture_ram_reservation_precedes_all_allocating_identity_hashes() {
    run_wide_common::with_fixture(1, |f, _| {
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
        let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
        let deny: TirMemoryReserverV1 = Arc::new(|role, _, _, _| {
            assert_eq!(role, "positive-wide-panel-capture-v2");
            Err("capture RAM denied".into())
        });
        let error = match capture_one_pass_v2(
            CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
            &runner,
            run_bounds(),
            &work,
            tempfile::tempfile().unwrap(),
            &deny,
        ) {
            Ok(_) => panic!("unreserved capture accepted"),
            Err(e) => e,
        };
        assert!(error.contains("RAM denied"));
        let spent = work.meter();
        assert_eq!(spent.hash_work, 0);
        assert!(spent.visits > 0);
        let second = match capture_one_pass_v2(
            CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
            &runner,
            run_bounds(),
            &work,
            tempfile::tempfile().unwrap(),
            &deny,
        ) {
            Ok(_) => panic!("failed attempt restarted"),
            Err(e) => e,
        };
        assert!(second.contains("already started"));
        assert_eq!(work.meter(), spent);
    });
}

#[path = "positive_wide_material_v2/stream_record_tests.rs"]
mod stream_record_tests;

#[path = "positive_wide_material_v2/loaded_bridge_tests.rs"]
mod loaded_bridge_tests;

#[path = "positive_wide_material_v2/trace_free_bridge_tests.rs"]
mod trace_free_bridge_tests;

#[cfg(unix)]
#[path = "positive_wide_material_v2/trained_roles.rs"]
mod trained_roles;

#[path = "positive_wide_material_v2/retained_handoff_tests.rs"]
mod retained_handoff_tests;

#[path = "positive_wide_material_v2/evaluation_diagnostics_tests.rs"]
mod evaluation_diagnostics_tests;
