//! Explicit local admission tests; none installs native state or authenticates a bond.
use super::*;
use crate::{
    config::params::{ForkActivation, Params, palw_t12_shipped_params},
    palw_class_admission_v2::PalwClassAdmissionError as E,
    palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2, PalwCourtParamsV2},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2},
    palw_tir_admission_v1::{
        PalwTirAdmissionRulesV1, palw_tir_class_record_v1, palw_tir_post_genesis_registration_v1,
        palw_tir_registration_preflight_at_v1, verify_class_admission_v10,
    },
    palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1},
    palw_tir_class_record_v2::{ProfiledTirClassRecordV2, TirClassRecordProfileLimitsV2},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1, palw_tir_class_registration_message_v1},
    palw_tir_close_size_v2::PalwTirCloseBoundV2,
    palw_tir_profile_resolver_v2::ResolvedTirJobProfileV2,
    palw_tir_step_v1::{PalwTirStepSpaceV1, PalwTirStepSpaceV2},
    palw_tir_v1::PalwTirFenceV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{
    DType, Ref, TensorType,
    builder::ProgramBuilder,
    program::{HISTORY_BOUND_V1_HELD, HISTORY_BOUND_V1_SMALL, LOGITS_I64_Q24_SCHEME_V2, MAX_PROGRAM_BYTES},
};

const AT: u64 = 1000;
fn params() -> Params {
    let mut params = palw_t12_shipped_params();
    params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    params.sync_palw_tir_v1();
    params
}
fn bundle() -> PalwConsensusParamsV2 {
    let params = params();
    let PalwConsensusMode::ConsensusV2(mut bundle) = params.palw_consensus_mode else { unreachable!() };
    bundle.court = PalwCourtParamsV2::with_cost_ceilings(1 << 30, 45, 2, 2_000_000, 1 << 30, 128).unwrap();
    bundle
}
fn rules() -> PalwTirAdmissionRulesV1 {
    let mut rules = PalwTirAdmissionRulesV1::at(&params(), AT).unwrap();
    // Only the pure function receives this local selection. No Params value or
    // preset is changed to arm native dispatch or the private environment.
    rules.fence.court_version = 2;
    rules
}
fn class(dtype: DType) -> (PalwTirClassV1, Hash64) {
    let mut p = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let embed = p.param("embed", DType::I8, &[4, 4], false);
    let head = p.param("head", DType::I8, &[4, 4], false);
    let fixed = p.fixed_state("last_row", DType::I8, &[4], -128, 127, false);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    pre.state_write(fixed, row);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let row = post.reshape_fixed(Ref::CarryIn(0), &[1, 4]);
    let product = post.matmul(row, head, DType::I64);
    let bounded = post.clamp(product, if dtype == DType::Idx { 0 } else { -64 }, 63, DType::I32);
    let narrow = post.reshape_fixed(bounded, &[4]);
    post.commit(narrow);
    let output = if dtype == DType::I32 {
        narrow
    } else {
        let output = post.cast(narrow, dtype);
        post.commit(output);
        output
    };
    let Ref::Node(logits) = output else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id =
        if dtype == DType::I64 { LOGITS_I64_Q24_SCHEME_V2 } else { crate::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes() };
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let commits = program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(18),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: vec![4; commits],
            state_tiles: vec![4],
        },
    };
    (class, Hash64::from_u64_word(29))
}
fn bond() -> PalwBondKeyV2 {
    PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([7; 64]), index: 0 })
}
fn construct(class: PalwTirClassV1, artifact: Hash64, canonical: PalwJobContextV2) -> Result<PalwConsensusObjectV2, E> {
    palw_tir_post_genesis_registration_v2(class, canonical, artifact, 0, 1 << 100, 1, AT + 10, bond(), vec![9; 16], 1 << 30)
}
fn registration() -> PalwConsensusObjectV2 {
    let (class, artifact) = class(DType::I64);
    let canonical = ResolvedTirJobProfileV2::new(&class, artifact, MAX_PROGRAM_BYTES).unwrap().ctx().unwrap();
    construct(class, artifact, canonical).unwrap()
}
fn admit(
    object: &PalwConsensusObjectV2,
) -> Result<(crate::palw_mode_v2::PalwClassCatalogEntryV2, crate::palw_tir_admission_v1::PalwTirClassRecordV1), E> {
    verify_tir_class_admission_v2(&bundle(), &rules(), object, &[], &[])
}
fn signing_message(object: &PalwConsensusObjectV2) -> Hash64 {
    let PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id,
        artifact_root,
        pwu_rule,
        initial_target,
        slash_value_per_pwu,
        share_permille,
        activation_daa,
        admission,
    } = object
    else {
        unreachable!()
    };
    palw_tir_class_registration_message_v1(
        Hash64::from_u64_word(90),
        *class_id,
        *share_permille,
        *activation_daa,
        &admission.registrant_bond,
        *artifact_root,
        *slash_value_per_pwu,
        *initial_target,
        pwu_rule,
        &admission.canonical,
        &admission.class,
    )
}

#[test]
fn original_wide_fixture_is_counted_and_records_original_full_schema_one_data() {
    let object = registration();
    let before = borsh::to_vec(&object).unwrap();
    assert_eq!(before[0], 61);
    let (entry, row) = admit(&object).unwrap();
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, pwu_rule, admission, .. } = &object else {
        unreachable!()
    };
    assert_eq!(entry.class_id, *class_id);
    assert_eq!(entry.artifact_root, *artifact_root);
    // Canonical P=7,D=2 runs eight pre rows, two pairs of post commits
    // and four Fixed checkpoints. Deepest P=1,D=64 runs64+128+32 leaves.
    assert_eq!(entry.canonical_step_leaf_count, 16);
    assert_eq!(entry.max_step_leaf_count, 224);
    assert_eq!(entry.court_cost.max_operand_count, 2);
    assert_eq!(*pwu_rule, PalwPwuRuleV2::DerivedV1 { pwu_per_inference: entry.canonical_step_leaf_count });
    let profile = ResolvedTirJobProfileV2::new(&admission.class, *artifact_root, MAX_PROGRAM_BYTES).unwrap();
    assert_eq!(Some(admission.canonical.clone()), profile.ctx());
    assert_eq!(admission.canonical.tokenizer_id, Hash64::default());
    assert_eq!(admission.canonical.cu_ruleset_id, Hash64::default());
    assert_ne!(admission.class.tokenizer_id, admission.canonical.tokenizer_id);
    let space = PalwTirStepSpaceV2::new(&admission.class).unwrap();
    assert_eq!(space.leaf_count_capped(&admission.canonical, 1 << 30).unwrap(), entry.canonical_step_leaf_count);
    assert!(entry.canonical_step_leaf_count <= entry.max_step_leaf_count);
    assert!(entry.court_cost.max_close_bytes >= 16 << 10);
    let expected = ProfiledTirClassRecordV2::build(
        &admission.class,
        *artifact_root,
        TirClassRecordProfileLimitsV2 { max_program_bytes: MAX_PROGRAM_BYTES, max_layout_entries: 8192 },
    )
    .unwrap();
    assert_eq!(&row, expected.storage_row());
    assert!(!row.facts.tiled);
    assert_eq!(row.program.as_ref(), &admission.class.program);
    assert_eq!(borsh::to_vec(&object).unwrap(), before);
}

#[test]
fn old_v1_factory_builder_gate_and_preflight_all_remain_closed_to_wide() {
    let object = registration();
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { artifact_root, admission, .. } = &object else { unreachable!() };
    assert!(palw_tir_class_record_v1(&admission.class, artifact_root).is_err());
    assert!(PalwTirStepSpaceV1::new(&admission.class).is_err());
    assert!(
        palw_tir_post_genesis_registration_v1(
            admission.class.clone(),
            admission.canonical.clone(),
            *artifact_root,
            0,
            1 << 100,
            1,
            AT + 10,
            bond(),
            vec![9; 16],
            1 << 30
        )
        .is_err()
    );
    let old_rules = PalwTirAdmissionRulesV1::at(&params(), AT).unwrap();
    assert!(verify_class_admission_v10(&bundle(), &old_rules, &object, &[], &[]).is_err());
    assert!(palw_tir_registration_preflight_at_v1(&params(), &bundle(), &object, AT, &[]).is_err());
}

#[test]
fn legacy_committable_outputs_keep_v10_semantics_and_are_not_new_wide_profiles() {
    for dtype in [DType::I8, DType::I16, DType::I32, DType::Idx] {
        let (class, artifact) = class(dtype);
        let id = class.class_id(&artifact);
        let facts = PalwTirJobFactsV1::of_class(&class, id).unwrap();
        let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&class).unwrap());
        let object = palw_tir_post_genesis_registration_v1(
            class.clone(),
            canonical.clone(),
            artifact,
            0,
            1 << 100,
            1,
            AT + 10,
            bond(),
            vec![9; 16],
            1 << 30,
        )
        .unwrap();
        let before = borsh::to_vec(&object).unwrap();
        let old_rules = PalwTirAdmissionRulesV1::at(&params(), AT).unwrap();
        let (_, record) = verify_class_admission_v10(&bundle(), &old_rules, &object, &[], &[]).unwrap();
        assert_eq!(record, palw_tir_class_record_v1(&class, &artifact).unwrap().0);
        assert_eq!(record.facts.class_id, id);
        assert!(admit(&object).is_err());
        assert!(construct(class, artifact, canonical).is_err());
        assert_eq!(borsh::to_vec(&object).unwrap(), before);
    }
}

#[test]
fn complete_canonical_identity_is_required_by_constructor_and_verifier() {
    let object = registration();
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { artifact_root, admission, .. } = &object else { unreachable!() };
    let original = signing_message(&object);
    for mutation in 0..7 {
        let mut canonical = admission.canonical.clone();
        match mutation {
            0 => canonical.tokenizer_id = admission.class.tokenizer_id,
            1 => canonical.cu_ruleset_id = Hash64::from_u64_word(91),
            2 => canonical.declared_prefill_tokens += 1,
            3 => canonical.network_id = b"testnet-12".to_vec(),
            4 => canonical.trace_scheme_id = crate::palw_step_refute::tiled_logits_scheme_id_v1(),
            5 => canonical.execution_seed[0] ^= 1,
            _ => canonical.job_id = Hash64::from_u64_word(92),
        }
        assert!(construct(admission.class.clone(), *artifact_root, canonical.clone()).is_err());
        let mut changed = object.clone();
        let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &mut changed else { unreachable!() };
        admission.canonical = canonical;
        assert_ne!(signing_message(&changed), original);
        assert!(admit(&changed).is_err(), "accepted canonical mutation {mutation}");
    }
}

#[test]
fn wrong_count_class_artifact_share_and_held_cannot_bypass_explicit_gate() {
    let original = registration();
    for mutation in 0..6 {
        let mut object = original.clone();
        let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, pwu_rule, share_permille, admission, .. } =
            &mut object
        else {
            unreachable!()
        };
        match mutation {
            0 => {
                let PalwPwuRuleV2::DerivedV1 { pwu_per_inference } = pwu_rule else { unreachable!() };
                *pwu_per_inference += 1;
            }
            1 => *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(1),
            2 => *class_id = Hash64::from_u64_word(93),
            3 => *artifact_root = Hash64::from_u64_word(94),
            4 => *share_permille = 1,
            _ => {
                let mut program = admission.class.decode_program().unwrap();
                program.history_bound = HISTORY_BOUND_V1_HELD;
                admission.class.program = program.encode();
                *class_id = admission.class.class_id(artifact_root);
                admission.canonical =
                    ResolvedTirJobProfileV2::new(&admission.class, *artifact_root, MAX_PROGRAM_BYTES).unwrap().ctx().unwrap();
                assert!(construct(admission.class.clone(), *artifact_root, admission.canonical.clone()).is_err());
            }
        }
        assert!(admit(&object).is_err(), "accepted mutation {mutation}");
    }
}

#[test]
fn local_rules_selection_does_not_arm_params_preflight_or_accept_other_object_kinds() {
    let object = registration();
    let params = params();
    assert_eq!(params.palw_tir_v1.unwrap().court_version, 1);
    let old_rules = PalwTirAdmissionRulesV1::at(&params, AT).unwrap();
    assert!(verify_tir_class_admission_v2(&bundle(), &old_rules, &object, &[], &[]).is_err());
    assert!(palw_tir_registration_preflight_at_v2(&params, &bundle(), &object, AT - 1, &[]).is_err());
    assert!(palw_tir_registration_preflight_at_v2(&params, &bundle(), &object, AT, &[]).is_err());
    let other = PalwConsensusObjectV2::PanelUnavailableQuorum { claim: Hash64::default(), receipts: vec![] };
    assert!(matches!(admit(&other), Err(E::NotARegistration)));
    use crate::palw_gen_class_v1::{
        OutputSpecV1, PalwGenAdmissionCarriageV1, PalwGenClassV1, PalwGenOffersV1, PalwGenProfileOffersV1,
    };
    // Deliberately invalid GEN payload: the variant boundary must refuse before
    // attempting to decode any pipeline or dispatch another class language.
    let gen_object = PalwConsensusObjectV2::ClassRegisteredGenV1 {
        class_id: Hash64::default(),
        artifact_root: Hash64::default(),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
        initial_target: 1,
        share_permille: 0,
        activation_daa: AT,
        admission: Box::new(PalwGenAdmissionCarriageV1 {
            class: PalwGenClassV1 {
                version: 1,
                profile: 0,
                pipeline: vec![],
                programs: vec![],
                layouts: vec![],
                output: OutputSpecV1 { kind: 0, shape: vec![], meta: vec![] },
                tokenizer_id: Hash64::default(),
                offers: PalwGenOffersV1 {
                    steps: vec![],
                    scalars: vec![],
                    max_prompt_tokens: 0,
                    max_negative_tokens: 0,
                    images: vec![],
                    max_source_tokens: 0,
                    source_token_floor: 0,
                    forced_prompt_prefix: vec![],
                    profile: PalwGenProfileOffersV1::None,
                },
            },
            registrant_bond: bond(),
            signature: vec![],
        }),
    };
    assert!(matches!(admit(&gen_object), Err(E::NotARegistration)));
}

#[test]
fn existing_resource_program_and_court_limits_remain_effective() {
    let object = registration();
    admit(&object).unwrap();
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &object else { unreachable!() };
    for mutation in 0..7 {
        let mut tight = rules();
        match mutation {
            0 => tight.fence.ceilings.max_program_bytes = admission.class.program.len() as u32 - 1,
            1 => tight.fence.ceilings.max_unrolled_nodes = 1,
            2 => tight.fence.ceilings.max_context = 63,
            3 => tight.fence.ceilings.max_macs_per_position = 1,
            4 => tight.fence.ceilings.max_state_bytes = 1,
            5 => tight.fence.ceilings.max_peak_live_bytes = 1,
            _ => tight.fence.ceilings.max_cone_work = 1,
        }
        assert!(verify_tir_class_admission_v2(&bundle(), &tight, &object, &[], &[]).is_err(), "ignored limit {mutation}");
    }
    for (close, macs, ladder) in [(1, 1 << 30, 1 << 30), (2_000_000, 1, 1 << 30), (2_000_000, 1 << 30, 2)] {
        let mut tight = bundle();
        tight.court = PalwCourtParamsV2::with_cost_ceilings(ladder, 45, 2, close, macs, 128).unwrap();
        assert!(verify_tir_class_admission_v2(&tight, &rules(), &object, &[], &[]).is_err());
    }
    let mut one_operand = bundle();
    one_operand.court = PalwCourtParamsV2::with_cost_ceilings(1 << 30, 45, 2, 2_000_000, 1 << 30, 1).unwrap();
    assert!(matches!(
        verify_tir_class_admission_v2(&one_operand, &rules(), &object, &[], &[]),
        Err(E::CourtCostExceedsCeiling { what: "operand count", got: 2, ceiling: 1 })
    ));
    let mut malformed = object;
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &mut malformed else { unreachable!() };
    admission.class.program.push(0);
    assert!(matches!(admit(&malformed), Err(E::TirProgram(_))));
}

fn bound(close_bytes: u64, root_claim_bytes: u64, dissected: bool) -> PalwTirCloseBoundV2 {
    PalwTirCloseBoundV2 { block: 0, node: 0, checkpoint: None, dissected, close_bytes, root_claim_bytes }
}
fn court(close: u64) -> PalwCourtParamsV2 {
    PalwCourtParamsV2::with_cost_ceilings(1 << 30, 45, 2, close, 1 << 30, 128).unwrap()
}

#[test]
fn cost_quote_preserves_both_components_and_enforces_empty_root_and_overflow_boundaries() {
    assert_eq!(coarse_close_bytes_v2(123, 456).unwrap(), 123 + (16 << 10) + 456);
    assert!(coarse_close_bytes_v2(u64::MAX, 0).is_err());
    assert!(coarse_close_bytes_v2(0, u64::MAX).is_err());
    for (coarse, serialized, expected) in [(50_000, 40_000, 50_000), (40_000, 50_000, 50_000)] {
        let bounds = [bound(serialized, 0, false)];
        assert_eq!(close_budget_v2(coarse, &bounds, &court(expected)).unwrap(), expected);
        assert!(close_budget_v2(coarse, &bounds, &court(expected - 1)).is_err());
    }
    assert!(close_budget_v2(0, &[], &court(50_000)).is_err());
    let root_cap = crate::palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1;
    assert_eq!(close_budget_v2(10, &[bound(20, root_cap, true)], &court(200_000)).unwrap(), 20);
    assert!(close_budget_v2(10, &[bound(20, root_cap + 1, true)], &court(200_000)).is_err());
    // A large court byte ceiling does not enlarge the structural32-chunk cap.
    let raw_capacity =
        crate::palw_state_v2::PALW_COURT_CLOSE_MAX_CHUNKS as u64 * crate::palw_state_v2::PALW_COURT_CLOSE_CHUNK_MAX_BYTES as u64;
    assert!(close_budget_v2(1, &[bound(raw_capacity + 1, 0, false)], &court(raw_capacity * 2)).is_err());
    let preimage = crate::palw_step_leg::PalwStepTileLeafV1 {
        version: 2,
        coord: crate::palw_step::PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
        value_count: 4,
        values_le: vec![0; 32],
    };
    let wide_leaf_bytes = borsh::to_vec(&preimage).unwrap().len() as u64;
    assert_eq!(wide_leaf_bytes, 26 + 32);
    let mut old_preimage = preimage;
    old_preimage.version = 1;
    old_preimage.values_le.truncate(16);
    let old_leaf_bytes = borsh::to_vec(&old_preimage).unwrap().len() as u64;
    assert_eq!(old_leaf_bytes, 26 + 16);
    // Synthetic surrounding whole-close frame: only the real leaf payload
    // width differs. A four-byte allowance cannot admit the wide DTO.
    let frame = 1000;
    let old_cap = frame + old_leaf_bytes;
    assert_eq!(close_budget_v2(0, &[bound(old_cap, 0, false)], &court(old_cap)).unwrap(), old_cap);
    assert!(close_budget_v2(0, &[bound(frame + wide_leaf_bytes, 0, false)], &court(old_cap)).is_err());
}

#[test]
fn sizing_exhaustion_and_unsupported_errors_remain_failures() {
    use crate::palw_tir_close_size_v1::{PALW_TIR_CLOSE_SIZING_OVER_CAP_V1, PALW_TIR_CLOSE_SIZING_WORK_CAP_V1};
    assert!(matches!(
        sizing_error_v2(PALW_TIR_CLOSE_SIZING_OVER_CAP_V1.into()),
        E::TirExceeds { limit: "IR close sizing work", value, cap, .. }
            if cap == PALW_TIR_CLOSE_SIZING_WORK_CAP_V1 && value == cap + 1
    ));
    for error in ["wide composite sizing unsupported", "wide sizing arithmetic overflow"] {
        assert!(matches!(sizing_error_v2(error.into()), E::TirLayout(message) if message.contains(error)));
    }
}

#[test]
fn original_wide_history_reduction_requires_a_sufficient_f7_window() {
    let mut p = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let history = p.hist_state("rows", DType::I32, &[4], 3, true);
    let carry = TensorType::fixed(DType::I32, &[4]);
    let mut pre = p.block("pre", vec![]);
    let row = pre.iota(DType::I32, &[misaka_palw_tir::Dim::Fixed(4)], 0, 1, 0);
    let pre = pre.finish(&[row]);
    let mut layer = p.block("layer", vec![carry.clone()]);
    let rows = layer.hist_append(history, Ref::CarryIn(0));
    // ReduceSum retains axis0 as a singleton: [H,4] -> [1,4].
    // Summing three full-I32 rows in I64 is universally within range.
    let sum = layer.reduce_sum(rows, 0, DType::I64);
    let bounded = layer.clamp(sum, -64, 63, DType::I32);
    let bounded = layer.reshape_fixed(bounded, &[4]);
    let Ref::Node(site) = bounded else { unreachable!() };
    let layer = layer.finish(&[bounded]);
    let mut post = p.block("post", vec![carry]);
    let output = post.cast(Ref::CarryIn(0), DType::I64);
    post.commit(output);
    let Ref::Node(logits) = output else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![layer], post, logits);
    program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(18),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 1,
            commit_tiles: vec![4; 3],
            state_tiles: vec![4],
        },
    };
    let original = class.clone();
    let artifact = Hash64::from_u64_word(39);
    let canonical = ResolvedTirJobProfileV2::new(&class, artifact, MAX_PROGRAM_BYTES).unwrap().ctx().unwrap();
    let object = construct(class, artifact, canonical).unwrap();
    let mut generous = rules();
    generous.model_court_window_active = false;
    generous.court = Some(crate::palw_class_admission_v2::PalwKaryCourtV1 {
        dissection_arity: 2,
        prompt_ids_form: generous.prompt_ids_form,
        window_court_daa: 100_000,
    });
    let (_, row) = verify_tir_class_admission_v2(&bundle(), &generous, &object, &[], &[]).unwrap();
    assert_eq!(row.dissected, vec![(layer, site)]);
    assert_eq!(row.program.as_ref(), &original.program);
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &object else { unreachable!() };
    assert_eq!(admission.class, original);
    let mut short = generous;
    short.court.as_mut().unwrap().window_court_daa = 1;
    assert!(matches!(verify_tir_class_admission_v2(&bundle(), &short, &object, &[], &[]), Err(E::CourtWindowTooShort { .. })));
}
