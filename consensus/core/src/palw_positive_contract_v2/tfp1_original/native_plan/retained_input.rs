//! Offline capacity diagnostic using retained actual Studio operands.
//! The prerequisite parent is CONSTRUCTED, never imported into a node. No
//! signature, native registration, admission, reservation or payment is made.
use crate as kaspa_consensus_core;
use kaspa_consensus_core::{
    Hash64,
    palw_class_semantics_v1::PalwClassSemanticsDeclarationV1,
    palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1,
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_positive_contract_v2::ProtectedPlanningDeclarationV3,
    palw_protected_original_quote_v1::{ProtectedOriginalQuoteMaterialV1, quote_protected_original_v1},
    palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_state_v2::*,
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_container_v1::{PalwTirContainerBindingV1, check_tir_container_binding_v1},
    palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1,
    palw_tokenizer_rendering_v2::{
        PalwDerivedTokenizerRenderingV2, PalwRootedTokenizerRegistrationRowV1, PalwTfp1TokenizerIntakePinsV2,
        PalwTokenizerRenderingBudgetV2, PalwVerifiedTokenizerRegistrationV1,
    },
};
use std::{fs, io::Read, path::Path};

fn read(base: &Path, name: &str, limit: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    fs::File::open(base.join(name)).unwrap().take(limit as u64 + 1).read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() <= limit, "{name} exceeds diagnostic input bound");
    bytes
}
fn hex(bytes: &[u8]) -> Vec<u8> {
    assert_eq!(bytes.len() % 2, 0);
    bytes.chunks_exact(2).map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap()).collect()
}
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

fn run() {
    let args: Vec<String> = std::iter::once("retained-input-native-quote".into())
        .chain(
            ["MISAKA_PLANNING_INPUT_DIR", "MISAKA_PLANNING_MATERIAL_DIR", "MISAKA_PLANNING_DECLARATION", "MISAKA_PLANNING_OUTPUT_DIR"]
                .map(|key| std::env::var(key).unwrap_or_else(|_| panic!("explicit diagnostic input required: {key}"))),
        )
        .collect();
    assert_eq!(args.len(), 5, "operand-directory material-directory declaration-json fresh-output-directory");
    let input = Path::new(&args[1]);
    let material_dir = Path::new(&args[2]);
    let output = Path::new(&args[4]);
    assert!(!output.exists());
    let original: PalwFreePromptCommitmentV3 = borsh::from_slice(&read(input, "commitment.borsh", 16 << 10)).unwrap();
    let class: PalwTirClassV1 = borsh::from_slice(&read(input, "class.borsh", 64 << 10)).unwrap();
    let semantics: PalwClassSemanticsDeclarationV1 = borsh::from_slice(&read(input, "class-semantics.borsh", 1024)).unwrap();
    let expected: PalwDerivedTokenizerRenderingV2 = borsh::from_slice(&read(input, "rendering-facts.borsh", 1024)).unwrap();
    let eog: PalwTokenTableEogOpeningV1 = borsh::from_slice(&read(input, "eog-opening.borsh", 4096)).unwrap();
    let raw = read(input, "tokenizer.json", 3 << 20);
    let binding: PalwTirContainerBindingV1 =
        borsh::from_slice(&read(material_dir, "container-binding.unsigned.borsh", 32 << 10)).unwrap();
    let PalwPublicExecutionBindingHeaderV1::Tir(header) = borsh::from_slice(&hex(&read(input, "public-binding.hex", 8192))).unwrap()
    else {
        panic!("TIR input required")
    };
    assert_eq!(class.class_id(&header.artifact_root), original.job.class_id);
    let (record, _) = palw_tir_class_record_v1(&class, &header.artifact_root).unwrap();
    let declaration = ProtectedPlanningDeclarationV3::decode_json_v3(&fs::read(&args[3]).unwrap()).unwrap();
    let declaration_id = declaration.identity_v3().unwrap();
    let mut material =
        ProtectedOriginalQuoteMaterialV1::decode_v1(&hex(&read(material_dir, "protected-material.hex", 32 << 10))).unwrap();
    assert_eq!(material.prompt, read(material_dir, "prompt.bin", 4096));
    assert_eq!(material.hf_descriptor, read(material_dir, "artifact.bin", 4096));
    material = material.with_planning_declaration_v3(&declaration).unwrap();
    let params = PalwStateParamsV2::new(100, 10, 40, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 600)
        .unwrap()
        .with_fp_exposure_ceiling(500)
        .unwrap()
        .with_escrow_backed_exposure_from_daa(Some(0))
        .with_rcore_plus_mirrors(Some(0), 12_900, Vec::new());
    let extras = PalwTransitionExtrasV1 {
        class_semantics_active: true,
        tir_container_identity_active: true,
        tir_public_conviction_active: true,
        staged_public_court: Some(PalwStagedPublicCourtConfigV1::network_v1()),
        prompt_ids_merkle: true,
        protected_planning_v3: Some(declaration),
        model_registry: Some(kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1 {
            globals: kaspa_consensus_core::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1,
            span_daa: 600,
            genesis_works: Default::default(),
            grace_until_daa: 0,
            admission_audit_period_daa: None,
            readiness_v2_active: false,
            bond_maturity: None,
        }),
        ..Default::default()
    };
    // Only a hypothetical prerequisite parent. Retain the original job and
    // execution identity byte-for-byte; no forged accepted claim row exists.
    let daa = original.job.anchor_daa.checked_add(1).unwrap();
    let before = PalwBlockContextV2 { block: h(5701), daa_score: daa - 3, blue_score: daa - 3, subsidy: 0 };
    let point = PalwBlockContextV2 { block: h(5702), daa_score: daa - 2, blue_score: daa - 2, subsidy: 0 };
    let bond = PalwBondKeyV2(original.job.executor_bond);
    let bound = semantics.as_bound_row(1);
    let container = check_tir_container_binding_v1(&binding, &original.job.class_id, &header.artifact_root, &record, 1).unwrap();
    let pins = PalwTfp1TokenizerIntakePinsV2 {
        class_id: original.job.class_id,
        container_root: container.container_root,
        semantics_id: bound.commitment(),
        inventory_id: h(5703),
    };
    let mut carried = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carried.classes.insert(
        original.job.class_id,
        PalwClassStateV2 {
            artifact_root: header.artifact_root,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(original.work_leaves),
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: None,
            fused_attention: false,
        },
    );
    carried.class_shares.insert(original.job.class_id, 1000);
    carried.tir_classes.insert(original.job.class_id, record);
    carried.class_semantics.insert(original.job.class_id, bound.clone());
    carried.staged_courts.insert(Key::TirLayout(original.job.class_id), Row::TirLayout(Box::new(class.layout)));
    carried.staged_courts.insert(Key::TirContainer(original.job.class_id), Row::TirContainer(Box::new(container)));
    carried.staged_courts.insert(
        Key::Inventory(original.job.tokenizer_id),
        Row::ArtifactInventory(PalwTokenizerArtifactInventoryCertificateV1 {
            artifact_hash: original.job.tokenizer_id,
            actual_bytes: raw.len() as u64,
            inventory_id: pins.inventory_id,
            opener: bond,
            opened_daa: 1,
            completed_daa: 2,
        }),
    );
    carried.last_point = Some(before);
    let parent = carried.into_state(&params, None).unwrap();
    let verified = PalwVerifiedTokenizerRegistrationV1::derive_v1(
        &parent,
        &extras,
        point.daa_score,
        pins,
        &raw,
        &eog,
        // The native V1 persisted identity has exactly these fixed limits.
        PalwTokenizerRenderingBudgetV2 {
            raw_bytes: 8 << 20,
            vocab: 262_144,
            key_events: 4 << 20,
            eog_ids: 4096,
            total_rendering_bytes: 8 << 20,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(verified.facts_v1(), &expected);
    // Fixed structural decoder, used only in this offline diagnostic. The
    // carriage consistency checks verify the rederived commitment/context.
    let row: PalwRootedTokenizerRegistrationRowV1 =
        borsh::from_slice(&borsh::to_vec(&(1u16, pins, bound, before, point, verified.commitment_v1(), verified.facts_v1())).unwrap())
            .unwrap();
    let mut carried = PalwStateCarriageV2::from_state(&parent);
    carried.staged_courts.insert(
        Key::VerifiedTokenizerRegistration(pins.class_id, pins.semantics_id),
        Row::VerifiedTokenizerRegistration(Box::new(row)),
    );
    carried.last_point = Some(point);
    let parent = carried.into_state(&params, None).unwrap();
    let root = parent.state_root();
    eprintln!("constructed prerequisite parent checked; querying native planner for {}", fp_claim_id_v3(&original));
    let result = quote_protected_original_v1(&parent, &params, &extras, daa, daa, &original, &header, &material);
    assert_eq!(root, parent.state_root());
    let diagnostic = match result {
        Ok(quote) => {
            serde_json::json!({"quoted":true,"claim":quote.claim.to_string(),"originalFeeSompi":quote.original_fee_sompi,"futureCarrierFeeSompi":quote.future_carrier_fee_sompi})
        }
        Err(error) => serde_json::json!({"quoted":false,"refusal":error}),
    };
    let report = serde_json::json!({
        "purpose":"offline actual-input planning diagnostic with constructed prerequisite parent",
        "originalClaim":fp_claim_id_v3(&original).to_string(),"originalBytesUnchanged":true,
        "quoteDaa":daa,"traceRetentionDaa":original.trace_retention_daa,"declarationId":declaration_id.to_string(),
        "hypotheticalParent":true,"nativeRegistration":false,"nativeAdmission":false,"capacitySelected":false,
        "parentUnchanged":true,"tokenizerRederived":true,"actualClaimCompletion":false,"diagnostic":diagnostic,
    });
    fs::create_dir(output).unwrap();
    fs::write(output.join("planning-diagnostic.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("{report}");
}
#[test]
#[ignore = "requires retained actual Studio operands and explicit diagnostic output paths; no live-chain admission"]
fn retained_studio_input_reaches_native_capacity_diagnostic() {
    std::thread::Builder::new().stack_size(64 << 20).spawn(run).unwrap().join().unwrap();
}
