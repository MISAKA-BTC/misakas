use super::*;
use crate::{
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1, PalwStagedCourtRowV1},
    palw_state_v2::{PalwBlockContextV2, PalwClassStateV2, PalwClassStatusV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateParamsV2},
    palw_tir_container_v1::{check_tir_container_binding_v1, test_binding_v1},
    palw_tir_public_claim_v1::palw_tir_public_signed_material_frame_fixture_v1,
};
use misaka_palw_positive_contract::{
    identity::{ChainScope, principal},
    public_one_honest::{HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1, hf_artifact_material_codec_v1, tir_inventory_material_codec_v1},
};

#[test]
fn ordinary_hf_artifact_resolves_accepted_pins_and_binds_manifest() {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([31; 32]);
    let fixture = palw_tir_public_signed_material_frame_fixture_v1(key.verification_key.as_ref());
    let frame = &fixture.frame;
    let fp = frame.original_v1();
    let source = frame.source_v1(&fixture.record, &fixture.layout, fixture.rules.maximum_leaves).unwrap();
    let header = source.header_v1();
    let job = &header.job_context;
    let class = PalwTirClassV1 {
        version: crate::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
        program: fixture.record.program.as_ref().clone(),
        layout: fixture.layout.clone(),
        tokenizer_id: fixture.record.tokenizer_id,
    };
    let class_id = fp.job.class_id;
    let inventory = header.artifact_root;
    assert_eq!(class.class_id(&inventory), class_id);
    let binding = test_binding_v1(&fixture.record, inventory, b"rooted HF artifact fixture");
    let container = check_tir_container_binding_v1(&binding, &class_id, &inventory, &fixture.record, 2).unwrap();
    let h = Hash64::from_u64_word;
    let params = PalwStateParamsV2::new(100, 10, 40, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 600).unwrap();
    // Minimal accepted snapshot for the exact registered class, using the
    // fixture's actual program/layout and a native-validated container row.
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carriage.last_point = Some(PalwBlockContextV2 { block: h(2), daa_score: 2, blue_score: 2, subsidy: 0 });
    carriage.classes.insert(
        class_id,
        PalwClassStateV2 {
            artifact_root: inventory,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(10),
            status: PalwClassStatusV2::Active,
            registered_daa: 1,
            registrant_bond: None,
            fused_attention: false,
        },
    );
    carriage.class_shares.insert(class_id, 1000);
    carriage.tir_classes.insert(class_id, fixture.record.clone());
    carriage
        .staged_courts
        .insert(PalwStagedCourtKeyV1::TirLayout(class_id), PalwStagedCourtRowV1::TirLayout(Box::new(fixture.layout.clone())));
    let pins = HfArtifactNativePinsV1 {
        class: Digest(class_id.as_bytes()),
        program: Digest(class.graph_ir_root().as_bytes()),
        tokenizer: Digest(class.tokenizer_id.as_bytes()),
        inventory: Digest(inventory.as_bytes()),
        container: Digest(container.container_root.as_bytes()),
        binding_manifest: Digest(container.manifest_digest.as_bytes()),
    };
    let make_descriptor =
        |pins, recipe| HfArtifactDescriptorV1::new_v1(pins, "HuggingFaceTB/SmolLM2-135M", [1; 20], Digest([2; 64]), recipe).unwrap();
    let descriptor = make_descriptor(pins, Digest([3; 64]));
    let rules = ManifestScopeRules {
        codec: hf_artifact_material_codec_v1(),
        max_bytes: HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u64,
        chunk_bytes: HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u32,
    };
    let unbound = carriage.clone().into_state(&params, None).unwrap();
    assert!(matches!(PublicTfp1HfArtifactV1::resolve_v1(&unbound, fp, &source, descriptor.clone(), rules), Err(ContractError::Scope)));
    carriage
        .staged_courts
        .insert(PalwStagedCourtKeyV1::TirContainer(class_id), PalwStagedCourtRowV1::TirContainer(Box::new(container)));
    let parent = carriage.into_state(&params, None).unwrap();
    let resolved = PublicTfp1HfArtifactV1::resolve_v1(&parent, fp, &source, descriptor.clone(), rules).unwrap();
    assert_eq!(resolved.descriptor_v1(), &descriptor);
    assert_eq!(resolved.entry_v1(), descriptor.manifest_entry_v1(rules).unwrap());
    assert_eq!(resolved.entry_v1().total_bytes as usize, descriptor.encode_v1().unwrap().len());
    assert_eq!(resolved.check_context_v1(fp, &source, &class), Ok(()));
    let different = Digest([0xA1; 64]);
    for foreign in [
        HfArtifactNativePinsV1 { class: different, ..pins },
        HfArtifactNativePinsV1 { program: different, ..pins },
        HfArtifactNativePinsV1 { tokenizer: different, ..pins },
        HfArtifactNativePinsV1 { inventory: different, ..pins },
        HfArtifactNativePinsV1 { container: different, ..pins },
        HfArtifactNativePinsV1 { binding_manifest: different, ..pins },
    ] {
        let substituted = make_descriptor(foreign, Digest([3; 64]));
        assert!(matches!(PublicTfp1HfArtifactV1::resolve_v1(&parent, fp, &source, substituted, rules), Err(ContractError::Scope)));
    }
    let mut foreign_class = class.clone();
    foreign_class.layout.max_context += 1;
    assert_eq!(resolved.check_context_v1(fp, &source, &foreign_class), Err(ContractError::Scope));
    let mut foreign_fp = fp.clone();
    foreign_fp.output_root = h(0xA2);
    assert_eq!(resolved.check_context_v1(&foreign_fp, &source, &class), Err(ContractError::Scope));
    let legacy_rules = ManifestScopeRules { codec: tir_inventory_material_codec_v1(), ..rules };
    assert!(matches!(
        PublicTfp1HfArtifactV1::resolve_v1(&parent, fp, &source, descriptor, legacy_rules),
        Err(ContractError::UnsupportedProfile)
    ));

    let id = Digest([0xA3; 64]);
    let scope = ChainScope { network: id, ruleset: id, branch: id };
    let mut original = OriginalCommit {
        scope,
        job: Digest(job.job_id.as_bytes()),
        execution: Digest(job.context_hash().as_bytes()),
        miner: principal(key.verification_key.as_ref()),
        registered_class: pins.class,
        artifact: pins.inventory,
        program: pins.program,
        prompt: id,
        input_tokens: Digest(job.prompt_token_ids_hash.as_bytes()),
        tokenizer: pins.tokenizer,
        execution_rules: Digest(job.cu_ruleset_id.as_bytes()),
        generation_seed: job.execution_seed,
        initial_state: id,
        dependencies: id,
        prefill: job.declared_prefill_tokens,
        decode: job.exact_decode_tokens,
        max_context: job.max_context_tokens,
        output: Digest(fp.output_root.as_bytes()),
        trace_root: Digest(header.committed_execution_root.as_bytes()),
        manifest_root: id,
        service_contract: id,
    };
    // Only the Artifact body is under test. The other scopes are structural
    // manifest entries, not evidence of data delivery or execution.
    let subjects = [
        original.artifact,
        original.program,
        original.prompt,
        original.input_tokens,
        original.tokenizer,
        original.execution_rules,
        original.initial_state,
        original.dependencies,
        original.trace_root,
        original.output,
    ];
    let placeholder = |subject| ManifestEntryV1 { subject, content_root: id, codec: id, total_bytes: 1, chunk_bytes: 1 };
    let mut manifest = ManifestV1 { service_contract: original.service_contract, entries: subjects.map(placeholder) };
    let mut scope_limits = [ManifestScopeRules { codec: id, max_bytes: 1, chunk_bytes: 1 }; MaterialScope::COUNT];
    scope_limits[MaterialScope::Artifact as usize] = rules;
    let other_layout = OriginalCommit { registered_class: different, ..original.clone() };
    let before = *manifest.entry(MaterialScope::Artifact);
    assert_eq!(resolved.set_manifest_entry_v1(&other_layout, &mut manifest), Err(ContractError::Scope));
    assert_eq!(*manifest.entry(MaterialScope::Artifact), before);
    resolved.set_manifest_entry_v1(&original, &mut manifest).unwrap();
    assert_eq!(*manifest.entry(MaterialScope::Artifact), resolved.entry_v1());
    assert_eq!(*manifest.entry(MaterialScope::Trace), placeholder(original.trace_root));
    let bytes = original
        .bind_manifest(&manifest, &scope_limits, rules.max_bytes + MaterialScope::COUNT as u64, ManifestV1::WIRE_BYTES)
        .unwrap();
    assert!(original.authenticate_manifest(&bytes, ManifestV1::WIRE_BYTES).is_ok());
    let another_recipe = PublicTfp1HfArtifactV1::resolve_v1(&parent, fp, &source, make_descriptor(pins, different), rules).unwrap();
    another_recipe.set_manifest_entry_v1(&original, &mut manifest).unwrap();
    assert!(matches!(
        original.authenticate_manifest(&borsh::to_vec(&manifest).unwrap(), ManifestV1::WIRE_BYTES),
        Err(ContractError::Identity)
    ));
}
