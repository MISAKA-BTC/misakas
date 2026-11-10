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
fn panel_opens_program_inputs_and_output_from_the_miners_bound_manifest() {
    use crate::miner::positive_v2::store::{StoreErrorV2, load_public_manifest_v1, store_public_manifest_v1};
    use crate::positive_panel_sources::{
        PanelSourceErrorR1, fetch_public_manifest_v1, fetch_public_replay_inputs_v1, fetch_public_replay_output_v1,
        open_public_input_token_tile_v1, open_public_output_tokens_v1, open_public_tir_program_v1, read_public_manifest_v1,
    };
    use crate::{
        evidence::fs::FsProvider,
        public_material::{MaterialProvider, public_original_manifest_path, public_original_replay_path},
    };
    use kaspa_consensus_core::palw_prompt_ids_v1::prompt_ids_opening_v1;
    use misaka_palw_positive_contract::public_one_honest::{ManifestV1, MaterialScope, OriginalCommit, tir_output_material_codec_v1};

    let fixture = Fixture::new();
    let result = &fixture.result;
    let form = PalwPromptIdsFormV1::MerkleV1;
    let prompt = vec![1u32; PALW_PROMPT_IDS_TILE_LEN as usize + 1];
    let prompt_text = "a".repeat(prompt.len());
    let tokenizer_json = br#"{"model":{"type":"BPE","vocab":{"b":0,"a":1},"merges":[]},"added_tokens":[]}"#;
    use kaspa_consensus_core::palw_positive_contract_v2::public_tokenizer_material_v1;
    use misaka_palw_positive_contract::public_one_honest::{
        prompt_text_material_codec_v1, prompt_text_material_v1, tokenizer_json_material_codec_v1,
    };
    let prompt_rules = ManifestScopeRules { codec: prompt_text_material_codec_v1(), max_bytes: 64, chunk_bytes: 64 };
    let tokenizer_rules = ManifestScopeRules { codec: tokenizer_json_material_codec_v1(), max_bytes: 256, chunk_bytes: 256 };
    let prompt_entry = prompt_text_material_v1(result.scope.ruleset, &prompt_text, prompt_rules).unwrap();
    let tokenizer_entry = public_tokenizer_material_v1(tokenizer_json, tokenizer_rules).unwrap();
    let mut context = fixture.context.clone();
    context.tokenizer_id = Hash64::from_bytes(tokenizer_entry.subject.0);
    context.declared_prefill_tokens = prompt.len() as u32;
    context.max_context_tokens = context.declared_prefill_tokens + context.exact_decode_tokens;
    context.prompt_token_ids_hash = prompt_token_ids_commitment_v1(form, &prompt).unwrap();
    let output_ids = vec![2u32; context.exact_decode_tokens as usize];
    let output_root = Digest(palw_attempt_output_root_v1(&context, &output_ids).as_bytes());
    let job = TirMinerJobV2 { class: &fixture.class, context: &context, prompt: &prompt, prompt_form: form };
    let mut original = OriginalCommit {
        scope: result.scope,
        job: result.job,
        execution: Digest(context.context_hash().as_bytes()),
        miner: result.miner,
        registered_class: result.registered_class,
        artifact: result.artifact,
        program: result.program,
        prompt: prompt_entry.subject,
        input_tokens: Digest(context.prompt_token_ids_hash.as_bytes()),
        tokenizer: tokenizer_entry.subject,
        execution_rules: result.execution_rules,
        generation_seed: result.generation_seed,
        initial_state: d(2),
        dependencies: result.dependencies,
        prefill: context.declared_prefill_tokens,
        decode: result.decode,
        max_context: context.max_context_tokens,
        output: output_root,
        trace_root: d(3),
        manifest_root: d(0),
        service_contract: result.service_contract,
    };
    let size = u32::try_from(fixture.class.program.len()).unwrap();
    let program_rules = ManifestScopeRules { codec: tir_program_material_codec_v1(), max_bytes: u64::from(size), chunk_bytes: size };
    let fixture_rules = ManifestScopeRules { codec: d(9), max_bytes: 1, chunk_bytes: 1 };
    let mut rules = [fixture_rules; MaterialScope::COUNT];
    rules[MaterialScope::Program as usize] = program_rules;
    rules[MaterialScope::Prompt as usize] = prompt_rules;
    rules[MaterialScope::Tokenizer as usize] = tokenizer_rules;
    let input_rules = ManifestScopeRules {
        codec: Digest(prompt_ids_form_id_v1(form).as_bytes()),
        max_bytes: u64::from(original.prefill) * 4,
        chunk_bytes: PALW_PROMPT_IDS_TILE_LEN * 4,
    };
    rules[MaterialScope::InputTokens as usize] = input_rules;
    let output_bytes = context.exact_decode_tokens * 4;
    let output_rules =
        ManifestScopeRules { codec: tir_output_material_codec_v1(), max_bytes: u64::from(output_bytes), chunk_bytes: output_bytes };
    rules[MaterialScope::Output as usize] = output_rules;
    let output_entry = |root| ManifestEntryV1 {
        subject: root,
        content_root: root,
        codec: output_rules.codec,
        total_bytes: output_rules.max_bytes,
        chunk_bytes: output_rules.chunk_bytes,
    };
    // The remaining scopes are placeholders and are never opened here.
    let fixture_entry =
        |subject| ManifestEntryV1 { subject, content_root: subject, codec: fixture_rules.codec, total_bytes: 1, chunk_bytes: 1 };
    let manifest = ManifestV1 {
        service_contract: original.service_contract,
        entries: [
            fixture_entry(original.artifact),
            job.public_program_material(program_rules).unwrap(),
            prompt_entry,
            job.public_input_token_material(input_rules).unwrap(),
            tokenizer_entry,
            fixture_entry(original.execution_rules),
            fixture_entry(original.initial_state),
            fixture_entry(original.dependencies),
            fixture_entry(original.trace_root),
            output_entry(original.output),
        ],
    };
    let limit = u64::from(size)
        + input_rules.max_bytes
        + output_rules.max_bytes
        + prompt_entry.total_bytes
        + tokenizer_entry.total_bytes
        + (MaterialScope::COUNT - 5) as u64;
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
    let mut oversized_original = original.clone();
    let mut oversized_manifest = manifest.clone();
    let program_index = MaterialScope::Program as usize;
    let oversized_bytes = misaka_palw_tir::program::MAX_PROGRAM_BYTES as u32 + 1;
    oversized_manifest.entries[program_index].total_bytes = u64::from(oversized_bytes);
    oversized_manifest.entries[program_index].chunk_bytes = oversized_bytes;
    let mut oversized_rules = rules;
    oversized_rules[program_index].max_bytes = u64::from(oversized_bytes);
    oversized_rules[program_index].chunk_bytes = oversized_bytes;
    let oversized_limit = limit + u64::from(oversized_bytes - size);
    let oversized_body = oversized_original.bind_manifest(&oversized_manifest, &oversized_rules, oversized_limit, 4096).unwrap();
    let oversized_checked = oversized_original
        .authenticate_manifest(&oversized_body, 4096)
        .unwrap()
        .decode(&oversized_original, &oversized_rules, oversized_limit)
        .unwrap();
    assert_eq!(
        fetch_public_replay_inputs_v1(&provider, &oversized_original, &oversized_checked, form),
        Err(PanelSourceErrorR1::Budget)
    );
    let fetched = fetch_public_manifest_v1(&provider, &original, &rules, limit, 4096);
    let overwrite = provider.put(&public_original_manifest_path(original.id()), &forged);
    let replay_manifest = fetched.as_ref().unwrap();
    let missing = fetch_public_replay_inputs_v1(&provider, &original, replay_manifest, form);
    let output_bytes: Vec<_> = output_ids.iter().flat_map(|id| id.to_le_bytes()).collect();
    for (scope, bytes) in [
        (MaterialScope::Program, fixture.class.program.as_slice()),
        (MaterialScope::Prompt, prompt_text.as_bytes()),
        (MaterialScope::Tokenizer, tokenizer_json.as_slice()),
        (MaterialScope::Output, output_bytes.as_slice()),
    ] {
        let path = public_original_replay_path(original.id(), scope).unwrap();
        crate::evidence::fs::write_provider_chunk(&store.join(&path), bytes).unwrap();
        assert!(provider.put(&path, bytes).is_err());
    }
    let replay = fetch_public_replay_inputs_v1(&provider, &original, replay_manifest, form);
    let output = fetch_public_replay_output_v1(&provider, &original, replay_manifest);
    let output_path = public_original_replay_path(original.id(), MaterialScope::Output).unwrap();
    let mut wrong_output_bytes = output_bytes;
    wrong_output_bytes[0] ^= 1;
    std::fs::write(store.join(output_path), wrong_output_bytes).unwrap();
    let altered_output = fetch_public_replay_output_v1(&provider, &original, replay_manifest);
    let path = public_original_replay_path(original.id(), MaterialScope::Prompt).unwrap();
    std::fs::write(store.join(path), "b".repeat(prompt_text.len())).unwrap();
    let altered = fetch_public_replay_inputs_v1(&provider, &original, replay_manifest, form);
    use kaspa_consensus_core::{
        palw_step::PalwStepCoordinateV1,
        palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_merkle_leaf_v1, step_tile_leaf_hash_v1},
        palw_tir_court_v1::PublicOriginalTraceIndexV1,
        palw_tir_step_v1::palw_tir_execution_root_v1,
    };
    let mut trace_original = original.clone();
    let ctx = Hash64::from_bytes(original.execution.0);
    let class = Hash64::from_bytes(original.registered_class.0);
    let tile = PalwStepTileLeafV1 {
        version: 1,
        coord: PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
        value_count: 1,
        values_le: 7u32.to_le_bytes().to_vec(),
    };
    let leaf_hash = step_tile_leaf_hash_v1(&ctx, &class, &tile);
    let trace = PublicOriginalTraceIndexV1 {
        full_logits_trace_root: Hash64::from_bytes([3; 64]),
        step_leaf_count: 1,
        step_merkle_root: step_merkle_leaf_v1(0, &leaf_hash),
    };
    trace_original.trace_root =
        Digest(palw_tir_execution_root_v1(&ctx, &trace.full_logits_trace_root, &class, 1, &trace.step_merkle_root).as_bytes());
    use crate::positive_panel_sources::{PublicTraceAcquisitionV1, check_public_replay_trace_v1};
    let total_bytes = PublicOriginalTraceIndexV1::WIRE_BYTES as u64 + 4;
    let limits = PublicTraceAcquisitionV1 { max_tiles: 1, max_wire_bytes: 1024 };
    let check = |index, total, limits| check_public_replay_trace_v1(&provider, &trace_original, index, &[leaf_hash], total, 4, limits);
    assert_eq!(check(&trace, total_bytes, limits), Err(PanelSourceErrorR1::Incomplete));
    let index_path = crate::public_material::public_original_trace_index_path(trace_original.id());
    crate::evidence::fs::write_provider_chunk(&store.join(index_path), &borsh::to_vec(&trace).unwrap()).unwrap();
    assert_eq!(check(&trace, total_bytes, limits), Err(PanelSourceErrorR1::Incomplete));
    let replayed = PublicOriginalTraceIndexV1 { step_merkle_root: Hash64::from_bytes([8; 64]), ..trace.clone() };
    assert_eq!(check(&replayed, total_bytes, limits), Err(PanelSourceErrorR1::Binding));
    let opening = PalwStepOpeningV1 { leaf_index: 0, leaf_hash, siblings: vec![] };
    let material = (opening, tile);
    let path = crate::public_material::public_original_trace_tile_path(trace_original.id(), 0);
    let mut bytes = borsh::to_vec(&material).unwrap();
    crate::evidence::fs::write_provider_chunk(&store.join(&path), &bytes).unwrap();
    let fetch = || crate::positive_panel_sources::fetch_public_trace_tile_v1(&provider, &trace_original, &trace, 0, 1, 4);
    assert_eq!(fetch(), Ok(material));
    assert_eq!(check(&trace, total_bytes, limits), Ok(()));
    assert_eq!(
        check_public_replay_trace_v1(&provider, &trace_original, &trace, &[Hash64::from_bytes([8; 64])], total_bytes, 4, limits),
        Err(PanelSourceErrorR1::Binding)
    );
    assert_eq!(check(&trace, total_bytes + 1, limits), Err(PanelSourceErrorR1::Binding));
    let short = PublicTraceAcquisitionV1 { max_wire_bytes: total_bytes, ..limits };
    assert_eq!(check(&trace, total_bytes, short), Err(PanelSourceErrorR1::Budget));
    *bytes.last_mut().unwrap() ^= 1;
    std::fs::write(store.join(path), bytes).unwrap();
    assert_eq!(fetch(), Err(PanelSourceErrorR1::Binding));
    assert_eq!(check(&trace, total_bytes, limits), Err(PanelSourceErrorR1::Binding));
    std::fs::remove_dir_all(&store).unwrap();
    assert!(matches!(missing, Err(PanelSourceErrorR1::Incomplete)));
    assert_eq!(replay.unwrap(), (fixture.plan.program.clone(), prompt.clone()));
    assert_eq!(output.unwrap(), output_ids);
    assert_eq!(altered_output, Err(PanelSourceErrorR1::Binding));
    assert!(matches!(altered, Err(PanelSourceErrorR1::Contract(ContractError::Identity))));
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
    use crate::positive_panel_sources::check_public_prompt_v1;
    assert_eq!(
        check_public_prompt_v1(&original, &checked, PalwPromptIdsFormV1::MerkleV1, prompt_text.as_bytes(), tokenizer_json),
        Ok(prompt.clone())
    );
    assert_eq!(
        check_public_prompt_v1(&original, &checked, PalwPromptIdsFormV1::Flat, prompt_text.as_bytes(), tokenizer_json),
        Err(PanelSourceErrorR1::Binding)
    );
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
    assert_eq!(open_public_output_tokens_v1(&original, &checked, &output_ids), Ok(()));
    let mut wrong_output = output_ids.clone();
    wrong_output[0] ^= 1;
    assert_eq!(open_public_output_tokens_v1(&original, &checked, &wrong_output), Err(PanelSourceErrorR1::Binding));

    use crate::positive_panel_sources::open_public_flat_input_tokens_v1;
    let form = PalwPromptIdsFormV1::Flat;
    context.prompt_token_ids_hash = prompt_token_ids_commitment_v1(form, &prompt).unwrap();
    original.input_tokens = Digest(context.prompt_token_ids_hash.as_bytes());
    original.execution = Digest(context.context_hash().as_bytes());
    original.output = Digest(palw_attempt_output_root_v1(&context, &output_ids).as_bytes());
    let job = TirMinerJobV2 { class: &fixture.class, context: &context, prompt: &prompt, prompt_form: form };
    let flat_rules = ManifestScopeRules {
        codec: Digest(prompt_ids_form_id_v1(form).as_bytes()),
        chunk_bytes: input_rules.max_bytes as u32,
        ..input_rules
    };
    let mut manifest = manifest;
    rules[MaterialScope::InputTokens as usize] = flat_rules;
    manifest.entries[MaterialScope::InputTokens as usize] = job.public_input_token_material(flat_rules).unwrap();
    manifest.entries[MaterialScope::Output as usize] = output_entry(original.output);
    let encoded = original.bind_manifest(&manifest, &rules, limit, 4096).unwrap();
    let flat = read_public_manifest_v1(&original, &rules, limit, 4096, encoded.len() as u64, encoded.as_slice()).unwrap();
    assert_eq!(check_public_prompt_v1(&original, &flat, form, prompt_text.as_bytes(), tokenizer_json), Ok(prompt.clone()));
    assert!(check_public_prompt_v1(&original, &flat, form, prompt_text.as_bytes(), b"{}").is_err());
    assert_eq!(open_public_output_tokens_v1(&original, &checked, &output_ids), Err(PanelSourceErrorR1::Binding));
    assert_eq!(open_public_output_tokens_v1(&original, &flat, &output_ids), Ok(()));
    assert_eq!(open_public_flat_input_tokens_v1(&original, &flat, &prompt), Ok(()));
    assert_eq!(open_public_flat_input_tokens_v1(&original, &flat, &prompt[..prompt.len() - 1]), Err(PanelSourceErrorR1::Binding));
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

#[cfg(feature = "positive-panel")]
#[test]
fn panel_replays_the_same_original_the_miner_executes_signs_and_publishes() {
    use crate::{
        evidence::fs::FsProvider,
        positive_panel_sources::{
            PanelSourceErrorR1, PanelSourceLimitsR1, PublicManifestAcquisitionV1, PublicTraceAcquisitionV1,
            acquire_or_request_public_manifest_v1, fetch_public_manifest_v1, fetch_public_replay_inputs_v1,
            fetch_public_replay_output_v1, replay_and_sign_drawn_public_wide_v1,
        },
        public_material::{MaterialProvider, artifact_leaf_path, prepare_public_manifest_response_v1},
        transport::{
            HttpProvider,
            server::{self, ServerConfig},
        },
    };
    use kaspa_consensus_core::{
        palw_positive_contract_v2::{PublicOriginalAdmissionV1, PublicOriginalReservationV1},
        palw_state_v2::PalwBlockContextV2,
    };
    use misaka_palw_positive_contract::{
        public_one_honest::*,
        reservations::RemainingCapacity,
        rows::{AcceptedOrigin, AggregateUsage, NativeHash64},
    };
    use misaka_palw_tir_exec::node::{
        positive_wide_material_v2::{WidePanelDiskReserverV2, WidePanelLimitsV2, WidePanelWorkV2},
        run_wide_q24_v2::TirWideRunLimitsV2,
    };

    let mut fixture = Fixture::new();
    let tokenizer = br#"{"model":{"type":"BPE","vocab":{"x":0,"a":1,"b":2,"y":3},"merges":[]},"added_tokens":[]}"#;
    let rules = [
        tir_inventory_material_codec_v1(),
        tir_program_material_codec_v1(),
        prompt_text_material_codec_v1(),
        Digest(prompt_ids_form_id_v1(PalwPromptIdsFormV1::Flat).as_bytes()),
        tokenizer_json_material_codec_v1(),
        tir_wide_rules_material_codec_v1(),
        tir_cold_initial_state_codec_v1(),
        tir_cold_dependencies_codec_v1(),
        tir_trace_material_codec_v1(),
        tir_output_material_codec_v1(),
    ]
    .map(|codec| ManifestScopeRules { codec, max_bytes: 1 << 16, chunk_bytes: 1 << 16 });
    let mut program = fixture.plan.program.clone();
    program.logits_scheme_id = kaspa_consensus_core::palw_logits_q24_v2::scheme_id().as_bytes();
    program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.dtype = DType::I64;
    fixture.plan = TirPlan::compile(&program).unwrap();
    fixture.class.program = program.encode();
    let tokenizer_entry = kaspa_consensus_core::palw_positive_contract_v2::public_tokenizer_material_v1(
        tokenizer,
        rules[MaterialScope::Tokenizer as usize],
    )
    .unwrap();
    fixture.class.tokenizer_id = Hash64::from_bytes(tokenizer_entry.subject.0);
    fixture.context.tokenizer_id = fixture.class.tokenizer_id;
    fixture.context.trace_scheme_id = kaspa_consensus_core::palw_logits_q24_v2::scheme_id();
    fixture.context.max_context_tokens = fixture.class.layout.max_context;
    fixture.refresh_execution_identity();

    let job = TirMinerJobV2 {
        class: &fixture.class,
        context: &fixture.context,
        prompt: &fixture.prompt,
        prompt_form: PalwPromptIdsFormV1::Flat,
    };
    let result = &fixture.result;
    let prompt = prompt_text_material_v1(result.scope.ruleset, "ab", rules[MaterialScope::Prompt as usize]).unwrap();
    let mut budget = fixture.contract.budget;
    budget.retained_bytes = 1 << 22;
    budget.unique_source_bytes = 1 << 22;
    budget.transmitted_bytes = 1 << 22;
    budget.retention_until = 10_100;
    let obligation = result.service_contract;
    let service_contract = public_original_service_contract_id_v1(obligation, &budget);
    let mut original = OriginalCommit {
        scope: result.scope,
        job: result.job,
        execution: result.execution,
        miner: result.miner,
        registered_class: result.registered_class,
        artifact: result.artifact,
        program: result.program,
        prompt: prompt.subject,
        input_tokens: result.input,
        tokenizer: tokenizer_entry.subject,
        execution_rules: result.execution_rules,
        generation_seed: result.generation_seed,
        initial_state: d(0),
        dependencies: d(0),
        prefill: result.prefill,
        decode: result.decode,
        max_context: fixture.context.max_context_tokens,
        output: d(0),
        trace_root: d(0),
        manifest_root: d(0),
        service_contract,
    };
    original.dependencies =
        tir_cold_dependencies_material_v1(&original, rules[MaterialScope::Dependencies as usize]).unwrap().0.subject;
    let held = AuthenticatedTirParamsV2::authenticate(&fixture.plan, &fixture.params, fixture.artifact, identity_limits()).unwrap();
    let root = std::env::temp_dir().join(format!("misaka-first-wide-original-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let store_root = root.join("published");
    let tape = |name: &str| std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(root.join(name)).unwrap();
    let disk: WidePanelDiskReserverV2 = Arc::new(|_, _, _| Ok(Box::new(())));
    let limits = WidePanelLimitsV2 {
        max_records: 1024,
        max_step_leaves: 1024,
        max_tape_bytes: 1 << 20,
        max_live_heap_bytes: 1 << 35,
        max_visits: 1 << 30,
        max_io_bytes: 1 << 24,
        max_hash_work: 1 << 30,
        max_proof_attempts: 1,
        max_provider_bytes: 1 << 20,
        max_provider_scratch: 1 << 16,
        max_material_bytes: 1 << 16,
        max_claim_bytes: 1 << 16,
        max_trace_frames: 1024,
        external_retained_bytes: 1 << 16,
    };
    let work = || WidePanelWorkV2::new(limits, fixture.context.shape_profile_id, fixture.context.context_hash(), &disk).unwrap();
    let run_limits = TirWideRunLimitsV2 {
        max_positions: 64,
        max_step_leaves: 1024,
        max_row_lanes: 16,
        max_work: 1 << 40,
        max_requested_bytes: 1 << 30,
    };
    let reserver: misaka_palw_tir_exec::node::memory::TirMemoryReserverV1 = Arc::new(|_, _, _, _| Ok(Box::new(())));
    let signs = Cell::new(0);
    let signed = run_and_publish_public_wide_original_v1(
        &held,
        &job,
        "ab",
        tokenizer,
        original,
        obligation,
        &budget,
        &rules,
        1 << 20,
        ManifestV1::WIRE_BYTES,
        &store_root,
        run_limits,
        &work(),
        tape("miner-tape"),
        &reserver,
        &fixture.key,
        |message, context| {
            signs.set(signs.get() + 1);
            assert_eq!(context, ORIGINAL_SIGN_CONTEXT);
            let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([62; 32]);
            Ok(*libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, message, context, [0; 32]).unwrap().as_ref())
        },
    )
    .unwrap();
    assert_eq!(signs.get(), 1);
    signed.original.authenticate(&signed.key, &signed.signature).unwrap();
    let store = FsProvider { id: "miner".into(), root: store_root.clone() };
    let server = server::start("127.0.0.1:0", store_root.clone(), ServerConfig::default()).unwrap();
    let provider = HttpProvider::new(server.url());
    let manifest = fetch_public_manifest_v1(&provider, &signed.original, &rules, 1 << 20, ManifestV1::WIRE_BYTES).unwrap();
    assert_eq!(
        fetch_public_replay_inputs_v1(&provider, &signed.original, &manifest, PalwPromptIdsFormV1::Flat).unwrap(),
        (program.clone(), fixture.prompt.clone())
    );
    assert_eq!(fetch_public_replay_output_v1(&provider, &signed.original, &manifest).unwrap(), vec![3, 3, 3]);
    let limits = PanelSourceLimitsR1 {
        max_units: 4,
        max_unit_bytes: 64,
        max_total_bytes: 256,
        max_class_bytes: 1 << 16,
        max_container_bytes: 1 << 16,
    };
    let artifact_root = Hash64::from_bytes(signed.original.artifact.0);
    let admitted = &signed.original;
    let admission = PublicOriginalAdmissionV1 {
        scope: admitted.scope,
        job: &fixture.context,
        class: &fixture.class,
        artifact: artifact_root,
        miner: admitted.miner,
        prompt: admitted.prompt,
        initial_state: admitted.initial_state,
        dependencies: admitted.dependencies,
        service_contract,
        max_manifest_bytes: ManifestV1::WIRE_BYTES,
        manifest_scope_limits: &rules,
        manifest_total_limit: 1 << 20,
    };
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64([3; 64]),
        accepted_tx_ordinal: 0,
        event_ordinal: 0,
        accepted_daa: 100,
        accepted_blue_score: 100,
    };
    let capacity = RemainingCapacity { transmitted_bytes: 1 << 20, retained_bytes: 1 << 20, additional_work: 0 };
    let zero = RemainingCapacity { transmitted_bytes: 0, additional_work: 0, retained_bytes: 0 };
    let reservation = PublicOriginalReservationV1 {
        origin,
        service_obligation: obligation,
        initial_supply: capacity,
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
        rescue: ProtectedRescueDemand {
            max_drawn_bonds: 1,
            per_drawn_bond: capacity,
            max_outsiders: 0,
            per_outsider: zero,
            defensive: zero,
        },
        usage: &AggregateUsage::default(),
        budget: &budget,
    };
    let accepted = admission.prepare(admitted, &signed.key, &signed.signature, &reservation).unwrap();
    let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([63; 32]);
    let panel_key = panel.verification_key.as_ref();
    let bond = PanelBond { transaction: d(45), index: 0 };
    let draw_point = PalwBlockContextV2 { block: h(101), daa_score: 101, blue_score: 101, subsidy: 0 };
    let members = [PublicDrawMemberV1 { bond, principal: principal(panel_key), rescue_reservation: d(46) }];
    let draw = accepted.prepare_draw(d(47), draw_point, &members, stateless_wide_replay_plan_v1()).unwrap();
    let request_origin =
        AcceptedOrigin { accepting_block: NativeHash64([5; 64]), accepted_daa: 102, accepted_blue_score: 102, ..origin };
    let acquire = |source: &dyn MaterialProvider, bond, should_sign: bool| {
        acquire_or_request_public_manifest_v1(
            source,
            admitted,
            &accepted,
            &draw,
            bond,
            obligation,
            &rules,
            1 << 20,
            ManifestV1::WIRE_BYTES,
            panel_key,
            |message, domain| {
                assert!(should_sign);
                assert_eq!(domain, MANIFEST_REQUEST_SIGN_CONTEXT);
                Ok(*libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, message, domain, [0; 32]).unwrap().as_ref())
            },
        )
    };
    assert!(
        matches!(acquire(&provider, bond, false).unwrap(), PublicManifestAcquisitionV1::Acquired(checked) if *checked == manifest)
    );
    let missing = FsProvider { id: "empty".into(), root: root.join("empty-manifest-provider") };
    std::fs::create_dir_all(&missing.root).unwrap();
    assert!(matches!(
        acquire(&missing, PanelBond { index: 1, ..bond }, false),
        Err(PanelSourceErrorR1::Contract(ContractError::Identity))
    ));
    let PublicManifestAcquisitionV1::Request(request_carrier) = acquire(&missing, bond, true).unwrap() else {
        panic!("a missing manifest must produce a request");
    };
    let SignedManifestRequestV1 { request, key: request_key, signature: request_signature } = *request_carrier;
    let request_context = ManifestRequestContext {
        original: admitted,
        claimant: request.claimant,
        signer: request.signer,
        inspection_scope: request.inspection_scope,
        supply_obligation: obligation,
        reservation: request.reservation,
        max_manifest_bytes: ManifestV1::WIRE_BYTES,
        manifest_limits: manifest_limits_id_v1(&rules, 1 << 20, ManifestV1::WIRE_BYTES),
        manifest_body_bytes_reserved: u64::from(ManifestV1::WIRE_BYTES),
        schedule: accepted.schedule,
        origin: request_origin,
    };
    let pending = accepted.verify_drawn_manifest_request(&draw, &request, &request_context, &request_key, &request_signature).unwrap();
    let miner_pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([62; 32]);
    let response =
        prepare_public_manifest_response_v1(&provider, &pending, admitted, admitted.miner, &signed.key, |message, domain| {
            assert_eq!(domain, MANIFEST_RESPONSE_SIGN_CONTEXT);
            Ok(*libcrux_ml_dsa::ml_dsa_87::sign(&miner_pair.signing_key, message, domain, [0; 32]).unwrap().as_ref())
        })
        .unwrap();
    let response_origin =
        AcceptedOrigin { accepting_block: NativeHash64([6; 64]), accepted_daa: 103, accepted_blue_score: 103, ..origin };
    let delivery =
        pending.verify_provider_response_bytes(&response, admitted.miner, admitted, response_origin, &rules, 1 << 20).unwrap();
    assert!(matches!(delivery.into_outcome(), ManifestDeliveryV1::Checked(checked) if checked == manifest));
    let other_plan = accepted.prepare_draw(d(47), draw_point, &members, d(48)).unwrap();
    let panel_signatures = Cell::new(0);
    let artifact_path = |index| artifact_leaf_path(&artifact_root, index);
    let replay = |name: &str, draw, bond| {
        replay_and_sign_drawn_public_wide_v1(
            &provider,
            &signed,
            draw,
            bond,
            &fixture.context,
            &fixture.class,
            &rules,
            1 << 20,
            ManifestV1::WIRE_BYTES,
            limits,
            &root.join(format!("{name}-artifact")),
            run_limits,
            &work(),
            tape(name),
            &reserver,
            PublicTraceAcquisitionV1 { max_tiles: 1024, max_wire_bytes: 1 << 20 },
            None,
            panel_key,
            |message, context| {
                panel_signatures.set(panel_signatures.get() + 1);
                assert_eq!(context, FULL_REPLAY_SIGN_CONTEXT);
                Ok(*libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, message, context, [0; 32]).unwrap().as_ref())
            },
        )
    };
    assert_eq!(replay("other-plan", &other_plan, bond), Err(ContractError::Scope.into()));
    assert_eq!(replay("other-bond", &draw, PanelBond { index: 1, ..bond }), Err(ContractError::Identity.into()));
    assert_eq!(replay("missing-artifact", &draw, bond), Err(PanelSourceErrorR1::Incomplete));
    for row in 0..4u32 {
        let operand = PalwArtifactOperandV1 {
            tensor_name: "embedding".into(),
            layer: None,
            row_start: row * 4,
            bytes: (row * 4 + 1..=row * 4 + 4).map(|value| value as u8).collect(),
        };
        store.put(&artifact_path(row), &borsh::to_vec(&operand).unwrap()).unwrap();
    }
    assert_eq!(panel_signatures.get(), 0);
    let approval = replay("panel-tape", &draw, bond).unwrap();
    let inclusion = AcceptedOrigin {
        accepting_block: NativeHash64([10; 64]),
        carrier_block: NativeHash64([11; 64]),
        carrier_transaction: NativeHash64([12; 64]),
        accepted_daa: 102,
        accepted_blue_score: 102,
        ..origin
    };
    let bytes = borsh::to_vec(&approval).unwrap();
    draw.verify_replay_approval_bytes(102, inclusion, &bytes).unwrap();
    let tile = crate::public_material::public_original_trace_tile_path(signed.original.id(), 0);
    std::fs::remove_file(store_root.join(tile)).unwrap();
    assert_eq!(replay("incomplete-tape", &draw, bond), Err(PanelSourceErrorR1::Incomplete));
    let mut corrupt = provider.get(&artifact_path(0), 64).unwrap().unwrap();
    *corrupt.last_mut().unwrap() ^= 1;
    store.put(&artifact_path(0), &corrupt).unwrap();
    assert_eq!(replay("corrupt-artifact", &draw, bond), Err(PanelSourceErrorR1::Binding));
    assert_eq!(panel_signatures.get(), 1);
    server.stop();
    std::fs::remove_dir_all(root).unwrap();
}
