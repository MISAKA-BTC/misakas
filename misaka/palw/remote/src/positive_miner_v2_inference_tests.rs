//! Concrete inference tests use ordinary execution and independently assembled
//! G14 inventory leaves. There is no Panel capture or native admission fixture.
use super::*;
use kaspa_consensus_core::{
    palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1},
    palw_attempt_v2::palw_attempt_job_v1,
    palw_positive_source_geometry_v2::{GeometryLimitsV2, StoredUnitV2, TirSourceGeometryV2},
    palw_prompt_ids_v1::prompt_token_ids_commitment_v1,
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tir_attempt_v1::{palw_tir_attempt_canonical_v1, palw_tir_attempt_job_for_anchor_of_v1, palw_tir_attempt_job_for_anchor_v1},
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1},
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    identity::{ChainScope, principal},
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder};
use misaka_palw_tir_exec::{Buf, TirRowSourceV1, TirRowsInMemoryV1};
use std::{cell::Cell, collections::BTreeMap, sync::Arc};

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn limits() -> InferenceLimitsV2 {
    InferenceLimitsV2 { max_prefill: 8, max_decode: 8, max_positions: 16 }
}
fn identity_limits() -> ParameterIdentityLimitsV2 {
    ParameterIdentityLimitsV2 { max_bytes: 16, max_leaves: 4, max_compiled_nodes: 1024, max_parameter_instances: 1024 }
}

#[test]
fn program_material_is_the_registered_graph_in_one_chunk() {
    let fixture = Fixture::new();
    let job = TirMinerJobV2 {
        class: &fixture.class,
        context: &fixture.context,
        prompt: &fixture.prompt,
        prompt_form: PalwPromptIdsFormV1::Flat,
    };
    let size = u32::try_from(fixture.class.program.len()).unwrap();
    let rules = ManifestScopeRules { codec: tir_program_material_codec_v1(), max_bytes: u64::from(size), chunk_bytes: size };
    let entry = job.public_program_material(rules).unwrap();
    assert_eq!(entry.subject, fixture.result.program);
    assert_eq!(entry.content_root, fixture.result.program);
    assert_eq!(entry.chunk_range(0), Ok(0..u64::from(size)));
    assert_eq!(job.public_program_material(ManifestScopeRules { chunk_bytes: size - 1, ..rules }), Err(ContractError::Budget));
    assert_eq!(job.public_program_material(ManifestScopeRules { codec: d(0), ..rules }), Err(ContractError::UnsupportedProfile));
}

#[cfg(feature = "positive-panel")]
#[test]
fn panel_opens_program_and_input_tiles_from_the_miners_bound_manifest() {
    use crate::miner::positive_v2::store::{StoreErrorV2, load_public_manifest_v1, store_public_manifest_v1};
    use crate::positive_panel_sources::{
        PanelSourceErrorR1, fetch_public_manifest_v1, open_public_input_token_tile_v1, open_public_tir_program_v1,
        read_public_manifest_v1,
    };
    use crate::{
        evidence::fs::FsProvider,
        public_material::{MaterialProvider, public_original_manifest_path},
    };
    use kaspa_consensus_core::palw_prompt_ids_v1::prompt_ids_opening_v1;
    use misaka_palw_positive_contract::public_one_honest::{ManifestV1, MaterialScope, OriginalCommit};

    let fixture = Fixture::new();
    let result = &fixture.result;
    let form = PalwPromptIdsFormV1::MerkleV1;
    let prompt = vec![1u32; PALW_PROMPT_IDS_TILE_LEN as usize + 1];
    let mut context = fixture.context.clone();
    context.declared_prefill_tokens = prompt.len() as u32;
    context.prompt_token_ids_hash = prompt_token_ids_commitment_v1(form, &prompt).unwrap();
    let job = TirMinerJobV2 { class: &fixture.class, context: &context, prompt: &prompt, prompt_form: form };
    let mut original = OriginalCommit {
        scope: result.scope,
        job: result.job,
        execution: result.execution,
        miner: result.miner,
        registered_class: result.registered_class,
        artifact: result.artifact,
        program: result.program,
        prompt: d(1),
        input_tokens: Digest(context.prompt_token_ids_hash.as_bytes()),
        tokenizer: result.tokenizer,
        execution_rules: result.execution_rules,
        generation_seed: result.generation_seed,
        initial_state: d(2),
        dependencies: result.dependencies,
        prefill: context.declared_prefill_tokens,
        decode: result.decode,
        max_context: result.max_context,
        output: result.output,
        trace_root: d(3),
        manifest_root: d(0),
        service_contract: result.service_contract,
    };
    let size = u32::try_from(fixture.class.program.len()).unwrap();
    let program_rules = ManifestScopeRules { codec: tir_program_material_codec_v1(), max_bytes: u64::from(size), chunk_bytes: size };
    let fixture_rules = ManifestScopeRules { codec: d(9), max_bytes: 1, chunk_bytes: 1 };
    let mut rules = [fixture_rules; MaterialScope::COUNT];
    rules[MaterialScope::Program as usize] = program_rules;
    let input_rules = ManifestScopeRules {
        codec: Digest(prompt_ids_form_id_v1(form).as_bytes()),
        max_bytes: u64::from(original.prefill) * 4,
        chunk_bytes: PALW_PROMPT_IDS_TILE_LEN * 4,
    };
    rules[MaterialScope::InputTokens as usize] = input_rules;
    // The remaining scopes are placeholders and are never opened here.
    let fixture_entry =
        |subject| ManifestEntryV1 { subject, content_root: subject, codec: fixture_rules.codec, total_bytes: 1, chunk_bytes: 1 };
    let manifest = ManifestV1 {
        service_contract: original.service_contract,
        entries: [
            fixture_entry(original.artifact),
            job.public_program_material(program_rules).unwrap(),
            fixture_entry(original.prompt),
            job.public_input_token_material(input_rules).unwrap(),
            fixture_entry(original.tokenizer),
            fixture_entry(original.execution_rules),
            fixture_entry(original.initial_state),
            fixture_entry(original.dependencies),
            fixture_entry(original.trace_root),
            fixture_entry(original.output),
        ],
    };
    let limit = u64::from(size) + input_rules.max_bytes + (MaterialScope::COUNT - 2) as u64;
    let encoded = original.bind_manifest(&manifest, &rules, limit, 4096).unwrap();
    let store = std::env::temp_dir().join(format!("misaka-public-manifest-{}", std::process::id()));
    std::fs::create_dir(&store).unwrap();
    let written = store_public_manifest_v1(&store, &original, &rules, limit, 4096, &encoded);
    let loaded = load_public_manifest_v1(&store, &original, 4096);
    let mut forged = encoded.clone();
    forged[0] ^= 1;
    let refused = store_public_manifest_v1(&store, &original, &rules, limit, 4096, &forged);
    let unchanged = load_public_manifest_v1(&store, &original, 4096);
    let provider = FsProvider::new(&store);
    let fetched = fetch_public_manifest_v1(&provider, &original, &rules, limit, 4096);
    let overwrite = provider.put(&public_original_manifest_path(original.id()), &forged);
    std::fs::remove_dir_all(&store).unwrap();
    written.unwrap();
    assert_eq!(refused, Err(StoreErrorV2::Authentication));
    assert_eq!(unchanged.unwrap(), encoded);
    assert!(overwrite.is_err());
    let encoded = loaded.unwrap();
    let len = encoded.len() as u64;
    let checked = read_public_manifest_v1(&original, &rules, limit, 4096, len, encoded.as_slice()).unwrap();
    assert_eq!(fetched.unwrap(), checked);
    let mut wrong_frame = encoded.as_slice();
    assert_eq!(
        read_public_manifest_v1(&original, &rules, limit, 4096, len + 1, &mut wrong_frame),
        Err(ContractError::NonCanonical.into())
    );
    assert_eq!(wrong_frame.len(), encoded.len());
    assert_eq!(
        read_public_manifest_v1(&original, &rules, limit, 4096, len, &encoded[..encoded.len() - 1]),
        Err(PanelSourceErrorR1::ManifestRead(std::io::ErrorKind::UnexpectedEof))
    );
    let bytes = &fixture.class.program;
    assert_eq!(open_public_tir_program_v1(&checked, bytes).unwrap(), fixture.plan.program);
    assert_eq!(open_public_tir_program_v1(&checked, &bytes[..bytes.len() - 1]), Err(PanelSourceErrorR1::Binding));
    let mut substituted = bytes.clone();
    substituted[0] ^= 1;
    assert_eq!(open_public_tir_program_v1(&checked, &substituted), Err(PanelSourceErrorR1::Binding));
    let mut tail = prompt_ids_opening_v1(&prompt, PALW_PROMPT_IDS_TILE_LEN).unwrap();
    assert_eq!(open_public_input_token_tile_v1(&original, &checked, 1, &tail).unwrap(), &[1]);
    assert_eq!(open_public_input_token_tile_v1(&original, &checked, 0, &tail), Err(PanelSourceErrorR1::Binding));
    tail.tile_ids[0] = 2;
    assert_eq!(open_public_input_token_tile_v1(&original, &checked, 1, &tail), Err(PanelSourceErrorR1::Binding));
}
fn model_params(plan: &TirPlan, first: i128) -> TirParams<'static> {
    let mut map = MapParams::default();
    let mut values: Vec<i128> = (1..=16).collect();
    values[0] = first;
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], values).unwrap());
    TirParams::from_map(plan, &map).unwrap()
}

struct Fixture {
    plan: TirPlan,
    params: TirParams<'static>,
    artifact: Digest,
    class: PalwTirClassV1,
    context: PalwJobContextV2,
    prompt: Vec<u32>,
    result: MinerResultCommit,
    admission: AdmissionContext,
    contract: MinerServiceContract,
    key: [u8; PUBKEY_BYTES],
}
impl Fixture {
    fn new() -> Self {
        Self::with_profile(true, true)
    }
    /// Actual existing job derivation over registered tiny-class facts. This
    /// data fixture does not resolve a native job origin or grant admission.
    fn canonical_attempt(prefill_draw: bool) -> Self {
        let mut f = Self::new();
        let artifact = Hash64::from_bytes(f.artifact.0);
        let (registered, _) = palw_tir_class_record_v1(&f.class, &artifact).unwrap();
        let canonical = palw_tir_attempt_canonical_v1(&f.class).unwrap();
        assert_eq!(canonical, (7, 2));
        let anchor = Hash64::from_bytes([82; 64]);
        let (context, prompt) =
            palw_tir_attempt_job_for_anchor_of_v1(&registered.facts, &anchor, canonical, PalwPromptIdsFormV1::Flat).unwrap();
        assert_eq!(
            palw_tir_attempt_job_for_anchor_v1(&f.class, registered.facts.class_id, canonical, &anchor, PalwPromptIdsFormV1::Flat),
            Some((context.clone(), prompt.clone()))
        );
        f.context = palw_attempt_job_v1(context, prefill_draw);
        f.prompt = prompt;
        assert_ne!(registered.tokenizer_id, Hash64::default());
        assert_eq!(f.context.tokenizer_id, Hash64::default());
        f.result.job = Digest(f.context.job_id.as_bytes());
        f.result.input = Digest(f.context.prompt_token_ids_hash.as_bytes());
        f.result.tokenizer = Digest(f.context.tokenizer_id.as_bytes());
        f.result.execution_rules = Digest(f.context.cu_ruleset_id.as_bytes());
        f.result.generation_seed = f.context.execution_seed;
        f.result.prefill = f.context.declared_prefill_tokens;
        f.result.decode = f.context.exact_decode_tokens;
        f.result.max_context = f.context.max_context_tokens;
        f.admission.expected_job = f.result.job;
        f.admission.expected_input = f.result.input;
        f.admission.tokenizer = f.result.tokenizer;
        f.admission.execution_rules = f.result.execution_rules;
        f.refresh_execution_identity();
        f
    }
    fn with_profile(all_committed: bool, tiled: bool) -> Self {
        let mut p = ProgramBuilder::new(4, 1 << 18);
        let embedding = p.param("embedding", DType::I8, &[4, 4], false);
        let mut pre = p.block("pre", vec![]);
        let x = pre.gather(embedding, Ref::Input(0), 0, 0);
        let carry = if all_committed {
            pre.commit(x);
            x
        } else {
            // finish() automatically commits carry-out nodes. Keep a genuinely
            // non-carry intermediate uncommitted, then carry its exact value.
            let wide = pre.cast(x, DType::I16);
            pre.clamp(wide, -128, 127, DType::I8)
        };
        let pre = pre.finish(&[carry]);
        let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
        let logits = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(logits);
        let Ref::Node(logits) = logits else { unreachable!() };
        let post = post.finish(&[]);
        let mut program = p.finish(pre, vec![], post, logits);
        program.logits_scheme_id = if tiled {
            tiled_logits_scheme_id_v1().as_bytes()
        } else {
            kaspa_consensus_core::palw_step_refute::flat_logits_scheme_id_v1().as_bytes()
        };
        let plan = TirPlan::compile(&program).unwrap();
        let params = model_params(&plan, 1);
        // Independent tiny-inventory expectation: four rows of four i8 bytes.
        // Do not use the production visitor/frontier/parameter authenticator.
        let leaves: Vec<_> = (0u32..4)
            .map(|row| {
                artifact_leaf_v1(&PalwArtifactOperandV1 {
                    tensor_name: "embedding".into(),
                    layer: None,
                    row_start: row * 4,
                    bytes: (row * 4 + 1..=row * 4 + 4).map(|x| x as u8).collect(),
                })
            })
            .collect();
        let artifact = Digest(artifact_root_v1(&leaves).unwrap().as_bytes());
        let post_block = program.schedule.post as usize;
        let logits_node = program.logits as usize;
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: program.encode(),
            tokenizer_id: h(18),
            layout: PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 64,
                checkpoint_interval: 2,
                h_tile: 1,
                commit_tiles: program
                    .blocks
                    .iter()
                    .enumerate()
                    .flat_map(|(bi, block)| {
                        block.nodes.iter().enumerate().filter(|(_, node)| node.commit).map(move |(ni, _)| {
                            if bi == post_block && ni == logits_node {
                                kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES as u32
                            } else {
                                4
                            }
                        })
                    })
                    .collect(),
                state_tiles: vec![],
            },
        };
        let prompt = vec![1, 2];
        let context = PalwJobContextV2 {
            version: 2,
            network_id: b"a2-private-test".to_vec(),
            job_id: h(23),
            job_nullifier: h(24),
            assignment_id: h(25),
            execution_seed: [26; 32],
            model_profile_id: h(27),
            runtime_manifest_hash: h(28),
            runtime_class_id: h(29),
            shape_profile_id: class.class_id(&Hash64::from_bytes(artifact.0)),
            trace_scheme_id: Hash64::from_bytes(program.logits_scheme_id),
            cu_ruleset_id: h(30),
            tokenizer_id: class.tokenizer_id,
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &prompt).unwrap(),
            declared_prefill_tokens: 2,
            exact_decode_tokens: 3,
            max_context_tokens: 128,
        };
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([62; 32]);
        let key = *pair.verification_key.as_ref();
        let mut result = MinerResultCommit {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
            job: Digest(context.job_id.as_bytes()),
            execution: Digest(context.context_hash().as_bytes()),
            miner: principal(&key),
            registered_class: Digest(context.shape_profile_id.as_bytes()),
            artifact,
            program: Digest(class.graph_ir_root().as_bytes()),
            input: Digest(context.prompt_token_ids_hash.as_bytes()),
            tokenizer: Digest(context.tokenizer_id.as_bytes()),
            execution_rules: Digest(context.cu_ruleset_id.as_bytes()),
            selector: greedy_selector_id(),
            generation_seed: context.execution_seed,
            temperature_q24: 0,
            prefill: context.declared_prefill_tokens,
            decode: context.exact_decode_tokens,
            max_context: context.max_context_tokens,
            output: d(99),
            dependencies: d(31),
            service_contract: d(0),
        };
        let contract = MinerServiceContract {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: result.case_id(),
            source_catalog: d(32),
            provider_delegations: d(33),
            budget: AggregateBudget {
                rounds: 1,
                unique_source_bytes: 4096,
                transmitted_bytes: 8192,
                additional_work: 100,
                retained_bytes: 8192,
                retention_until: 100,
                fee: 10,
                witness_attempts: 1,
            },
            funding_consent: d(34),
            deadline: 90,
        };
        result.service_contract = contract.id();
        let admission = AdmissionContext {
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
        Self { plan, params, artifact, class, context, prompt, result, admission, contract, key }
    }
    fn refresh_execution_identity(&mut self) {
        self.context.shape_profile_id = self.class.class_id(&Hash64::from_bytes(self.artifact.0));
        self.result.registered_class = Digest(self.context.shape_profile_id.as_bytes());
        self.result.program = Digest(self.class.graph_ir_root().as_bytes());
        self.result.execution = Digest(self.context.context_hash().as_bytes());
        self.admission.registered_class = self.result.registered_class;
        self.admission.program = self.result.program;
        self.admission.expected_execution = self.result.execution;
        self.contract.case = self.result.case_id();
        self.result.service_contract = self.contract.id();
    }
    fn run(
        &self,
        held: &AuthenticatedTirParamsV2<'_, '_>,
        signs: &Cell<u32>,
    ) -> Result<(MinerResultCommit, [u8; SIGNATURE_BYTES], Vec<u32>), MinerErrorV2> {
        self.run_with_limits(held, signs, limits())
    }
    fn run_with_limits(
        &self,
        held: &AuthenticatedTirParamsV2<'_, '_>,
        signs: &Cell<u32>,
        local_limits: InferenceLimitsV2,
    ) -> Result<(MinerResultCommit, [u8; SIGNATURE_BYTES], Vec<u32>), MinerErrorV2> {
        infer_and_sign_tir_result_v2(
            &self.admission,
            self.result.clone(),
            &self.contract,
            held,
            TirMinerJobV2 { class: &self.class, context: &self.context, prompt: &self.prompt, prompt_form: PalwPromptIdsFormV1::Flat },
            local_limits,
            1,
            &self.key,
            |message, context| {
                signs.set(signs.get() + 1);
                assert_eq!(context, RESULT_SIGN_CONTEXT);
                let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([62; 32]);
                Ok(*libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, message, context, [0; 32]).unwrap().as_ref())
            },
        )
    }
}

#[test]
fn bound_tir_infers_real_output_then_signs_only_the_result() {
    let f = Fixture::new();
    assert!(f.plan.program.blocks.iter().all(|b| b.nodes.iter().all(|n| n.commit)));
    let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
    assert_eq!(held.artifact(), f.artifact);
    assert_eq!(held.bytes_hashed(), 16);
    let signs = Cell::new(0);
    let (result, signature, tokens) = f.run(&held, &signs).unwrap();
    assert_eq!(tokens, vec![3, 3, 3]);
    assert_eq!(result.output, Digest(palw_attempt_output_root_v1(&f.context, &tokens).as_bytes()));
    assert_ne!(result.output, f.result.output); // template output is never trusted.
    assert_eq!(signs.get(), 1);
    result.authenticate(&f.key, &signature).unwrap();
    assert_eq!(result.service_contract, f.contract.id());
}

#[test]
fn canonical_attempt_context_tokenizer_zero_is_distinct_from_registered_class_and_still_runs_and_serves_g14() {
    for prefill_draw in [false, true] {
        let f = Fixture::canonical_attempt(prefill_draw);
        assert_ne!(f.class.tokenizer_id, Hash64::default());
        assert_eq!(f.context.tokenizer_id, Hash64::default());
        assert_eq!(f.result.tokenizer, Digest([0; 64]));
        assert_eq!(f.prompt.len(), 7);
        let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
        let signs = Cell::new(0);
        let (result, signature, generated) = f.run(&held, &signs).unwrap();
        assert_eq!(generated, vec![3; f.context.exact_decode_tokens as usize]);
        assert_eq!(signs.get(), 1);
        result.authenticate(&f.key, &signature).unwrap();
        assert_eq!(result.output, Digest(palw_attempt_output_root_v1(&f.context, &generated).as_bytes()));
        assert_eq!(result.tokenizer, Digest([0; 64]));

        let artifact = Hash64::from_bytes(f.artifact.0);
        let (registered, _) = palw_tir_class_record_v1(&f.class, &artifact).unwrap();
        let geometry = TirSourceGeometryV2::derive(
            &f.class,
            &registered,
            artifact,
            &f.context,
            &result,
            PalwPromptIdsFormV1::Flat,
            GeometryLimitsV2 {
                max_program_bytes: 1 << 20,
                max_parameter_instances: 4,
                max_inventory_leaves: 4,
                max_units_per_range: 4,
            },
        )
        .unwrap();
        let operands: Vec<_> = (0u32..4)
            .map(|row| PalwArtifactOperandV1 {
                tensor_name: "embedding".into(),
                layer: None,
                row_start: row * 4,
                bytes: (row * 4 + 1..=row * 4 + 4).map(|x| x as u8).collect(),
            })
            .collect();
        let request = SourceRequest {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: result.case_id(),
            contract: result.service_contract,
            source: result.artifact,
            kind: SourceKind::ArtifactInventory,
            first: 0,
            end: 1,
        };
        let opening = open_artifact_leaf_v1(&operands, 0).unwrap();
        let raw = borsh::to_vec(&opening).unwrap();
        assert_eq!(raw.len(), 166);
        let (index, unit) = geometry.decode_g14_response_v2(&request, &raw).unwrap();
        assert_eq!(index, 0);
        assert_eq!(unit, StoredUnitV2::Artifact(opening));
        assert_eq!(unit.sizes().unwrap(), (4, 166));
        let input_request = SourceRequest { source: result.input, kind: SourceKind::Input, ..request };
        let (_, input) = geometry.decode_g14_response_v2(&input_request, &borsh::to_vec(&f.prompt).unwrap()).unwrap();
        assert_eq!(input, StoredUnitV2::FlatInput(f.prompt.clone()));
    }
}

#[test]
fn canonical_context_or_result_tokenizer_substitution_does_not_reach_signer() {
    for attack in 0..3 {
        let mut f = Fixture::canonical_attempt(true);
        match attack {
            0 => f.context.tokenizer_id = f.class.tokenizer_id,
            1 => f.result.tokenizer = Digest(f.class.tokenizer_id.as_bytes()),
            _ => f.class.tokenizer_id = h(83),
        }
        let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
        let signs = Cell::new(0);
        assert!(f.run(&held, &signs).is_err(), "tokenizer substitution {attack}");
        assert_eq!(signs.get(), 0);
    }
}

#[test]
fn authenticated_weight_identity_is_reused_without_per_claim_rehash_or_capture() {
    let f = Fixture::new();
    let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
    let signs = Cell::new(0);
    let first = f.run(&held, &signs).unwrap();
    let second = f.run(&held, &signs).unwrap();
    assert_eq!(first, second);
    assert_eq!(signs.get(), 2);
    assert_eq!(held.bytes_hashed(), 16); // a load statistic, not per-run hashing.
}

#[test]
fn parameter_byte_leaf_root_and_completeness_bounds_fail_closed() {
    let f = Fixture::new();
    let mut cap = identity_limits();
    cap.max_bytes = 15;
    assert!(matches!(AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, cap), Err(MinerErrorV2::Budget)));
    let mut cap = identity_limits();
    cap.max_leaves = 3;
    assert!(matches!(AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, cap), Err(MinerErrorV2::Budget)));
    assert!(matches!(
        AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, d(88), identity_limits()),
        Err(MinerErrorV2::Binding)
    ));
    let different = model_params(&f.plan, 2);
    assert!(matches!(
        AuthenticatedTirParamsV2::authenticate(&f.plan, &different, f.artifact, identity_limits()),
        Err(MinerErrorV2::Binding)
    ));
    let absent = TirParams::new(&f.plan);
    assert!(matches!(
        AuthenticatedTirParamsV2::authenticate(&f.plan, &absent, f.artifact, identity_limits()),
        Err(MinerErrorV2::Binding)
    ));
}

#[test]
fn external_row_sources_are_not_assumed_authenticated_from_a_root_declaration() {
    let f = Fixture::new();
    let source = Arc::new(TirRowsInMemoryV1::new(BTreeMap::from([((0, None), (Buf::I8((1..=16).collect()), 4, 4))])));
    let mut params = TirParams::new(&f.plan);
    params.serve_rows(&f.plan, source.clone(), &|_, _| true).unwrap();
    assert!(matches!(
        AuthenticatedTirParamsV2::authenticate(&f.plan, &params, f.artifact, identity_limits()),
        Err(MinerErrorV2::Contract(ContractError::UnsupportedProfile))
    ));
    assert_eq!(source.counts().whole_reads, 0);
    assert_eq!(source.counts().rows_gathered, 0);
}

#[test]
fn changed_job_input_program_seed_selector_temperature_counts_or_scope_never_reach_signer() {
    for attack in 0..16 {
        let mut f = Fixture::new();
        match attack {
            0 => f.prompt[0] ^= 1,
            1 => f.class.program[0] ^= 1,
            2 => f.context.execution_seed[0] ^= 1,
            3 => f.result.generation_seed[0] ^= 1,
            4 => f.result.selector = d(77),
            5 => f.result.temperature_q24 = 1,
            6 => f.result.prefill += 1,
            7 => f.result.decode += 1,
            8 => f.result.max_context += 1,
            9 => f.result.scope.branch = d(77),
            10 => f.context.network_id.push(1),
            11 => f.context.assignment_id = h(77),
            12 => f.result.tokenizer = d(77),
            13 => f.result.execution_rules = d(77),
            14 => f.context.trace_scheme_id = h(77),
            _ => f.context.version = 1,
        }
        let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
        let signs = Cell::new(0);
        assert!(f.run(&held, &signs).is_err(), "attack {attack}");
        assert_eq!(signs.get(), 0, "attack {attack} reached signer");
    }
}

#[test]
fn unsupported_local_profile_and_wrong_admitted_metadata_are_refused_before_signing() {
    for (all_committed, tiled) in [(false, true), (true, false)] {
        let f = Fixture::with_profile(all_committed, tiled);
        assert_eq!(
            f.plan.program.blocks.iter().all(|b| b.nodes.iter().all(|n| n.commit)),
            all_committed,
            "profile fixture must actually have the requested commit coverage"
        );
        assert_eq!(
            f.class.layout.commit_tiles.len(),
            f.plan.program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count()
        );
        let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
        let signs = Cell::new(0);
        let outcome = f.run(&held, &signs);
        assert!(
            matches!(outcome, Err(MinerErrorV2::Contract(ContractError::UnsupportedProfile))),
            "profile all_committed={all_committed}, tiled={tiled}, actual error={:?}",
            outcome.as_ref().err()
        );
        assert_eq!(signs.get(), 0);
    }
    for field in 0..6 {
        let mut f = Fixture::new();
        match field {
            0 => f.admission.expected_input = d(78),
            1 => f.admission.expected_execution = d(78),
            2 => f.admission.expected_job = d(78),
            3 => f.admission.artifact = d(78),
            4 => f.admission.program = d(78),
            _ => f.admission.scope.network = d(78),
        }
        let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
        let signs = Cell::new(0);
        assert!(f.run(&held, &signs).is_err());
        assert_eq!(signs.get(), 0);
    }
}

#[test]
fn caller_mutated_compiled_fields_are_ignored_in_favor_of_canonical_program() {
    let f = Fixture::new();
    for mutation in 0..3 {
        let mut supplied = f.plan.clone();
        match mutation {
            0 => supplied.blocks.clear(),
            1 => {
                supplied.occurrences.clear();
                supplied.slot_bases.clear();
            }
            _ => {
                supplied.param_instances.clear();
                supplied.reads_token = false;
            }
        }
        let held = AuthenticatedTirParamsV2::authenticate(&supplied, &f.params, f.artifact, identity_limits()).unwrap();
        let signs = Cell::new(0);
        let (result, signature, tokens) = f.run(&held, &signs).unwrap();
        assert_eq!(tokens, vec![3, 3, 3]);
        assert_eq!(result.output, Digest(palw_attempt_output_root_v1(&f.context, &tokens).as_bytes()));
        result.authenticate(&f.key, &signature).unwrap();
        assert_eq!(signs.get(), 1);
    }
}

#[test]
fn malformed_canonical_schedule_and_shape_overflow_are_errors_instead_of_panics() {
    let f = Fixture::new();
    for mutation in 0..2 {
        let mut supplied = f.plan.clone();
        if mutation == 0 {
            supplied.program.schedule.post = u8::MAX;
        } else {
            supplied.program.params[0].shape = vec![u32::MAX; 3];
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            AuthenticatedTirParamsV2::authenticate(&supplied, &f.params, f.artifact, identity_limits())
        }));
        assert!(outcome.is_ok(), "malformed schedule/shape panicked: mutation {mutation}");
        assert!(outcome.unwrap().is_err(), "malformed program was accepted: mutation {mutation}");
    }
}

#[test]
fn excessive_metadata_and_noncanonical_layout_are_refused_even_with_consistent_ids() {
    for attack in 0..4 {
        let mut f = Fixture::new();
        match attack {
            0 => f.context.network_id.resize(kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES + 1, b'x'),
            1 => f.class.program.resize(misaka_palw_tir::program::MAX_PROGRAM_BYTES + 1, 0),
            2 => f.class.layout.commit_tiles.push(4),
            _ => f.class.layout.state_tiles.push(4),
        }
        f.refresh_execution_identity();
        let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
        let signs = Cell::new(0);
        assert!(f.run(&held, &signs).is_err(), "excessive metadata attack {attack}");
        assert_eq!(signs.get(), 0);
    }
}

#[test]
fn local_inference_and_compilation_metadata_budgets_are_enforced() {
    let f = Fixture::new();
    for parameter_limited in [false, true] {
        let mut cap = identity_limits();
        if parameter_limited {
            cap.max_parameter_instances = 0;
        } else {
            cap.max_compiled_nodes = 1;
        }
        assert!(matches!(AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, cap), Err(MinerErrorV2::Budget)));
    }
    let held = AuthenticatedTirParamsV2::authenticate(&f.plan, &f.params, f.artifact, identity_limits()).unwrap();
    for which in 0..3 {
        let mut local = limits();
        match which {
            0 => local.max_prefill = 1,
            1 => local.max_decode = 2,
            _ => local.max_positions = 3,
        }
        let signs = Cell::new(0);
        assert_eq!(f.run_with_limits(&held, &signs, local), Err(MinerErrorV2::Budget));
        assert_eq!(signs.get(), 0);
    }
}
