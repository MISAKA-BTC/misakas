//! Structural public-API tests. Hashes establish data identity, not arithmetic correctness.
use kaspa_consensus_core::{
    Hash64,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_step_leg::{PalwStepTileLeafV1, step_merkle_root_v1, step_tile_leaf_hash_v1},
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_step_v1::{
        PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepLegBuilderV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1,
        palw_tir_lane_values_v1, palw_tir_leaf_preimage_v1, verify_tir_binding_v1,
        wide_v2::{
            PALW_TIR_STEP_LEAF_VERSION_V2, PalwTirStepLegBuilderV2, PalwTirStepSpaceV2, palw_tir_leaf_preimage_v2,
            palw_tir_leaf_values_v2, verify_tir_binding_v2,
        },
    },
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{DType, Ref, TensorType, TirProgramV1, builder::ProgramBuilder, program::LOGITS_I64_Q24_SCHEME_V2};

fn program(wide: bool) -> TirProgramV1 {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 4], false);
    let scale = wide.then(|| p.konst(DType::I64, &[4], &[1 << 32, 1 << 33, 1 << 34, 1 << 35]));
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let narrow = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(narrow);
    let logits = if let Some(scale) = scale {
        let wide = post.mul(narrow, scale, DType::I64);
        post.commit(wide);
        wide
    } else {
        narrow
    };
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = if wide { LOGITS_I64_Q24_SCHEME_V2 } else { tiled_logits_scheme_id_v1().as_bytes() };
    misaka_palw_tir::validate::validate(&program).unwrap();
    // The I8 carry bounds survive the cast. Even 127 * 2^35 fits I64.
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    program
}

fn fixture(wide: bool) -> (PalwTirClassV1, Hash64, PalwJobContextV2) {
    let program = program(wide);
    let class = PalwTirClassV1 {
        version: 1,
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 16,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: vec![4; if wide { 3 } else { 2 }],
            state_tiles: vec![],
        },
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let artifact = Hash64::from_u64_word(29);
    let z = Hash64::from_bytes([0; 64]);
    let job = PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_u64_word(41),
        job_nullifier: z,
        assignment_id: z,
        execution_seed: [2; 32],
        model_profile_id: z,
        runtime_manifest_hash: z,
        runtime_class_id: z,
        shape_profile_id: class.class_id(&artifact),
        trace_scheme_id: if wide { kaspa_consensus_core::palw_logits_q24_v2::scheme_id() } else { tiled_logits_scheme_id_v1() },
        cu_ruleset_id: z,
        tokenizer_id: z,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &[0, 1]).unwrap(),
        declared_prefill_tokens: 2,
        exact_decode_tokens: 2,
        max_context_tokens: 16,
    };
    (class, artifact, job)
}

fn values(dtype: DType) -> [i128; 4] {
    match dtype {
        DType::I8 => [127, -128, 0, 1],
        DType::I32 => [7, -8, 9, -10],
        DType::I64 => [7 << 32, -(8 << 33), 9 << 34, -(10 << 35)],
        _ => panic!("unexpected fixture dtype"),
    }
}

// Freeze the existing byte/hash grammar independently of producer encoding and tree helpers.
fn hash(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        h.update(part);
    }
    Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap())
}
fn manual_leaf_hash(ctx: Hash64, class_id: Hash64, leaf: &PalwStepTileLeafV1) -> Hash64 {
    let mut bytes = leaf.version.to_le_bytes().to_vec();
    bytes.extend_from_slice(ctx.as_byte_slice());
    bytes.extend_from_slice(class_id.as_byte_slice());
    for n in [leaf.coord.call_index, leaf.coord.node_slot, leaf.coord.position, leaf.coord.tile_index, leaf.value_count] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes.extend_from_slice(&(leaf.values_le.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&leaf.values_le);
    hash(b"misaka-palw/step-tile-leaf/v1", &[&bytes])
}
fn manual_tree(leaves: &[Hash64]) -> Hash64 {
    let mut level: Vec<_> = leaves
        .iter()
        .enumerate()
        .map(|(i, leaf)| hash(b"misaka-palw/step-merkle-leaf/v1", &[&(i as u64).to_le_bytes(), leaf.as_byte_slice()]))
        .collect();
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|pair| match pair {
                [left, right] => hash(b"misaka-palw/step-merkle-node/v1", &[left.as_byte_slice(), right.as_byte_slice()]),
                [odd] => *odd,
                _ => unreachable!(),
            })
            .collect();
    }
    level[0]
}
fn manual_execution(ctx: Hash64, trace: Hash64, class_id: Hash64, count: u64, merkle: Hash64) -> Hash64 {
    let step = hash(
        b"misaka-palw/step-leg/v1",
        &[&1u16.to_le_bytes(), ctx.as_byte_slice(), class_id.as_byte_slice(), &count.to_le_bytes(), merkle.as_byte_slice()],
    );
    hash(b"misaka-palw/tir/execution-root/v1", &[ctx.as_byte_slice(), trace.as_byte_slice(), step.as_byte_slice()])
}
fn binding() -> PalwTirStepBindingV1 {
    let (class, artifact_root, job_context) = fixture(true);
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    let mut builder = PalwTirStepLegBuilderV2::new(&space, &job_context, class.class_id(&artifact_root), 7).unwrap();
    while let Some(leaf) = builder.next_leaf() {
        builder.push(&values(leaf.dtype)).unwrap();
    }
    let (step_leaf_count, step_merkle_root) = builder.finish().unwrap();
    let full_logits_trace_root = Hash64::from_u64_word(51);
    let committed_execution_root = manual_execution(
        job_context.context_hash(),
        full_logits_trace_root,
        class.class_id(&artifact_root),
        step_leaf_count,
        step_merkle_root,
    );
    PalwTirStepBindingV1 {
        version: 1,
        job_context,
        class,
        artifact_root,
        full_logits_trace_root,
        step_leaf_count,
        step_merkle_root,
        committed_execution_root,
    }
}

#[test]
fn original_wide_program_has_mixed_width_geometry_and_independent_hash_roots() {
    assert_eq!(PALW_TIR_STEP_LEAF_VERSION_V2, 2);
    let (class, artifact, job) = fixture(true);
    let original = class.program.clone();
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    let program = class.decode_program().unwrap();
    let info = misaka_palw_tir::validate::validate(&program).unwrap();
    let from_program = PalwTirStepSpaceV2::from_program(program, info, class.layout.clone()).unwrap();
    assert_eq!(space.program().encode(), original);
    assert_eq!(space.layout(), &class.layout);
    assert_eq!(space.info(), from_program.info());
    assert_eq!(space.leaf_count_capped(&job, 7).unwrap(), 7);
    assert!(space.leaf_count_capped(&job, 6).is_err());
    assert_eq!(space.maximum_leaf_payload_bytes_v2(&job).unwrap(), 32);
    let expected = [
        (0, 0, 0, 0, DType::I8),
        (1, 0, 1, 0, DType::I8),
        (1, 0, 1, 1, DType::I32),
        (1, 0, 1, 2, DType::I64),
        (2, 1, 0, 0, DType::I8),
        (2, 1, 0, 1, DType::I32),
        (2, 1, 0, 2, DType::I64),
    ];
    let class_id = class.class_id(&artifact);
    let mut builder = PalwTirStepLegBuilderV2::new(&space, &job, class_id, 7).unwrap();
    let mut hashes = vec![];
    for (index, &(position, call, relative_position, slot, dtype)) in expected.iter().enumerate() {
        let leaf = space.leaf_at(&job, index as u64).unwrap();
        assert_eq!((leaf.position, leaf.coord.node_slot, leaf.dtype), (position, slot, dtype));
        assert_eq!(leaf.coord.call_index, call);
        assert_eq!(leaf.coord.position, relative_position);
        assert_eq!(leaf.coord.tile_index, 0);
        assert_eq!(leaf.value_count, 4);
        assert_eq!(space.leaf_index(&job, &leaf.coord), Some(index as u64));
        assert_eq!(from_program.leaf_at(&job, index as u64), Some(leaf));
        let lanes = values(dtype);
        let encoded = palw_tir_leaf_preimage_v2(&space, &job, &leaf, &lanes).unwrap();
        let expected_bytes: Vec<_> = lanes
            .iter()
            .flat_map(|n| if dtype == DType::I64 { (*n as i64).to_le_bytes().to_vec() } else { (*n as i32).to_le_bytes().to_vec() })
            .collect();
        assert_eq!(encoded.values_le, expected_bytes);
        assert_eq!(encoded.version, if dtype == DType::I64 { PALW_TIR_STEP_LEAF_VERSION_V2 } else { 1 });
        assert_eq!(palw_tir_leaf_values_v2(&space, &job, &leaf, &encoded).unwrap(), lanes);
        let hash = manual_leaf_hash(job.context_hash(), class_id, &encoded);
        assert_eq!(hash, step_tile_leaf_hash_v1(&job.context_hash(), &class_id, &encoded));
        hashes.push(hash);
        assert_eq!(builder.next_leaf(), Some(leaf));
        assert_eq!(builder.push(&lanes).unwrap(), leaf);
    }
    assert!(space.leaf_at(&job, 7).is_none());
    assert!(builder.push(&[0; 4]).is_err());
    let (count, root) = builder.finish().unwrap();
    assert_eq!(count, 7);
    assert_eq!(root, manual_tree(&hashes));
    assert_eq!(root, step_merkle_root_v1(&hashes).unwrap());
    let trace = Hash64::from_u64_word(51);
    assert_eq!(
        manual_execution(job.context_hash(), trace, class_id, count, root),
        palw_tir_execution_root_v1(&job.context_hash(), &trace, &class_id, count, &root)
    );
    assert_eq!(class.program, original);
}

#[test]
fn legacy_narrow_preimages_and_root_stay_byte_identical_and_v1_refuses_wide() {
    let (class, artifact, job) = fixture(false);
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let class_id = class.class_id(&artifact);
    let mut builder = PalwTirStepLegBuilderV1::new(&space, &job, class_id, 5).unwrap();
    let mut hashes = vec![];
    for index in 0..5 {
        let leaf = space.leaf_at(&job, index).unwrap();
        let lanes = values(leaf.dtype);
        let encoded = palw_tir_leaf_preimage_v1(&leaf, &lanes).unwrap();
        let bytes: Vec<u8> = if leaf.dtype == DType::I8 {
            vec![127, 0, 0, 0, 128, 255, 255, 255, 0, 0, 0, 0, 1, 0, 0, 0]
        } else {
            vec![7, 0, 0, 0, 248, 255, 255, 255, 9, 0, 0, 0, 246, 255, 255, 255]
        };
        assert_eq!(encoded.version, 1);
        assert_eq!(encoded.values_le, bytes);
        hashes.push(manual_leaf_hash(job.context_hash(), class_id, &encoded));
        builder.push(&lanes).unwrap();
    }
    assert_eq!(builder.finish().unwrap(), (5, manual_tree(&hashes)));
    let wide = binding();
    assert!(PalwTirStepSpaceV1::new(&wide.class).is_err());
    let p = wide.class.decode_program().unwrap();
    let info = misaka_palw_tir::validate::validate(&p).unwrap();
    assert!(PalwTirStepSpaceV1::from_program(p, info, wide.class.layout.clone()).is_err());
    // Public legacy fields must not provide a bypass around V1 construction.
    let mut mutated_legacy = PalwTirStepSpaceV1::new(&class).unwrap();
    mutated_legacy.program = wide.class.decode_program().unwrap();
    assert!(PalwTirStepLegBuilderV1::new(&mutated_legacy, &job, class_id, 7).is_err());
    assert!(verify_tir_binding_v1(&wide, 7).is_err());
    assert!(palw_tir_lane_values_v1(DType::I64, &[0; 8]).is_err());
    assert!(palw_tir_lane_values_v1(DType::I128, &[0; 16]).is_err());
    assert!(PalwTirStepSpaceV2::new(&class).is_err());
}

#[test]
fn canonical_final_i64_format_rejects_out_of_range_and_forged_leaf_dtos() {
    let (class, _, job) = fixture(true);
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    let leaf = space.leaf_at(&job, 3).unwrap();
    assert!(palw_tir_leaf_preimage_v1(&leaf, &[0; 4]).is_err());
    let boundary = [i64::MIN as i128, i64::MAX as i128, -1, 0];
    let encoded = palw_tir_leaf_preimage_v2(&space, &job, &leaf, &boundary).unwrap();
    assert_eq!(palw_tir_leaf_values_v2(&space, &job, &leaf, &encoded).unwrap(), boundary);
    for invalid in [i64::MAX as i128 + 1, i64::MIN as i128 - 1, i128::MAX, i128::MIN] {
        assert!(palw_tir_leaf_preimage_v2(&space, &job, &leaf, &[invalid, 0, 0, 0]).is_err());
    }
    assert!(palw_tir_leaf_preimage_v2(&space, &job, &leaf, &[0; 3]).is_err());
    let mut forged = vec![];
    let mut altered = leaf;
    altered.index = 6;
    forged.push(altered);
    let mut altered = leaf;
    altered.coord.node_slot = 1;
    forged.push(altered);
    let mut altered = leaf;
    altered.coord.position = 2;
    forged.push(altered);
    let mut altered = leaf;
    altered.coord.tile_index = 1;
    forged.push(altered);
    let mut altered = leaf;
    altered.position = 2;
    forged.push(altered);
    let mut altered = leaf;
    altered.dtype = DType::I128;
    forged.push(altered);
    let mut altered = leaf;
    altered.dtype = DType::I32;
    forged.push(altered);
    let mut altered = leaf;
    altered.value_count = 3;
    forged.push(altered);
    let mut altered = leaf;
    if let PalwTirLeafKindV1::Commit { ref mut first_element, .. } = altered.kind {
        *first_element = 1;
    }
    forged.push(altered);
    for altered in forged {
        assert!(palw_tir_leaf_preimage_v2(&space, &job, &altered, &[0; 4]).is_err(), "accepted {altered:?}");
        assert!(palw_tir_leaf_values_v2(&space, &job, &altered, &encoded).is_err(), "decoded {altered:?}");
    }
    let mut malformed = vec![];
    let mut altered = encoded.clone();
    altered.version = 1;
    malformed.push(altered);
    let mut altered = encoded.clone();
    altered.coord.node_slot = 1;
    malformed.push(altered);
    let mut altered = encoded.clone();
    altered.value_count = 3;
    malformed.push(altered);
    let mut altered = encoded.clone();
    altered.values_le.truncate(16);
    malformed.push(altered);
    let mut altered = encoded.clone();
    altered.values_le.push(0);
    malformed.push(altered);
    for altered in malformed {
        assert!(palw_tir_leaf_values_v2(&space, &job, &leaf, &altered).is_err());
    }
    let narrow = space.leaf_at(&job, 0).unwrap();
    assert!(palw_tir_leaf_preimage_v2(&space, &job, &narrow, &[128, 0, 0, 0]).is_err());
    // A raw disclosed out-of-range narrow value remains readable for subsequent adjudication.
    let mut raw = palw_tir_leaf_preimage_v2(&space, &job, &narrow, &[0; 4]).unwrap();
    raw.values_le[..4].copy_from_slice(&128i32.to_le_bytes());
    assert_eq!(palw_tir_leaf_values_v2(&space, &job, &narrow, &raw).unwrap(), [128, 0, 0, 0]);
    raw.version = 2;
    assert!(palw_tir_leaf_values_v2(&space, &job, &narrow, &raw).is_err());
}

#[test]
fn profile_metadata_and_builder_admission_cannot_be_substituted() {
    let (class, artifact, job) = fixture(true);
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    let p = class.decode_program().unwrap();
    let actual_info = misaka_palw_tir::validate::validate(&p).unwrap();
    let mut other_info = actual_info.clone();
    // Validation metadata contains history geometry rather than every node's
    // type. Forge that geometry explicitly instead of assuming another program
    // necessarily has different metadata.
    assert_eq!(actual_info.blocks[0].window, None);
    other_info.blocks[0].window = Some(2);
    assert_ne!(other_info, actual_info);
    assert!(PalwTirStepSpaceV2::from_program(p.clone(), other_info, class.layout.clone()).is_err());
    let mut wrong = class.clone();
    wrong.version = 2;
    assert!(PalwTirStepSpaceV2::new(&wrong).is_err());
    let mut wrong = p.clone();
    wrong.blocks[wrong.schedule.post as usize].nodes[wrong.logits as usize].out.dtype = DType::I128;
    let mut wrong_class = class.clone();
    wrong_class.program = wrong.encode();
    assert!(PalwTirStepSpaceV2::new(&wrong_class).is_err());
    let mut wrong = program(false);
    wrong.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
    wrong_class.program = wrong.encode();
    assert!(PalwTirStepSpaceV2::new(&wrong_class).is_err());
    let mut wrong = p.clone();
    let post = &mut wrong.blocks[wrong.schedule.post as usize];
    post.nodes.push(post.nodes[0].clone());
    wrong_class.program = wrong.encode();
    assert!(PalwTirStepSpaceV2::new(&wrong_class).is_err());
    let class_id = class.class_id(&artifact);
    assert!(PalwTirStepLegBuilderV2::new(&space, &job, Hash64::from_u64_word(99), 7).is_err());
    assert!(PalwTirStepLegBuilderV2::new(&space, &job, class_id, 6).is_err());
    assert!(PalwTirStepLegBuilderV2::new(&space, &job, class_id, 7).unwrap().finish().is_err());
    let mut builder = PalwTirStepLegBuilderV2::new(&space, &job, class_id, 7).unwrap();
    assert!(builder.push(&[128, 0, 0, 0]).is_err());
    assert_eq!(builder.next_leaf().unwrap().index, 0);
    builder.push(&values(DType::I8)).unwrap();
    assert_eq!(builder.next_leaf().unwrap().index, 1);
    for change in 0..4 {
        let mut foreign = job.clone();
        match change {
            0 => foreign.version = 3,
            1 => foreign.trace_scheme_id = tiled_logits_scheme_id_v1(),
            2 => foreign.exact_decode_tokens = 0,
            _ => foreign.max_context_tokens = 3,
        }
        assert!(space.job_shape(&foreign).is_err());
        assert!(space.leaf_at(&foreign, 0).is_none());
        assert!(PalwTirStepLegBuilderV2::new(&space, &foreign, class_id, 7).is_err());
    }
}

#[test]
fn explicit_v2_validator_keeps_binding_v1_and_rejects_identity_root_and_count_changes() {
    let binding = binding();
    let verified = verify_tir_binding_v2(&binding, 7).unwrap();
    assert_eq!(binding.version, 1);
    assert_eq!(verified.class_id(), binding.class.class_id(&binding.artifact_root));
    assert_eq!(verified.context_hash(), binding.job_context.context_hash());
    assert_eq!(verified.space().program().encode(), binding.class.program);
    assert_eq!(verified.job(), verified.space().job_shape(&binding.job_context).unwrap());
    assert!(verify_tir_binding_v2(&binding, 6).is_err());
    for change in 0..11 {
        let mut bad = binding.clone();
        let foreign = Hash64::from_u64_word(91);
        match change {
            0 => bad.version = 2,
            1 => bad.job_context.version = 3,
            2 => bad.job_context.trace_scheme_id = tiled_logits_scheme_id_v1(),
            3 => bad.job_context.job_id = foreign,
            4 => bad.job_context.shape_profile_id = foreign,
            5 => bad.class.tokenizer_id = foreign,
            6 => bad.artifact_root = foreign,
            7 => bad.step_leaf_count += 1,
            8 => bad.step_merkle_root = foreign,
            9 => bad.full_logits_trace_root = foreign,
            _ => bad.committed_execution_root = foreign,
        }
        assert!(verify_tir_binding_v2(&bad, 8).is_err(), "accepted mutation {change}");
    }
    // Even a self-consistent root cannot turn a noncanonical count into valid geometry.
    let mut bad_count = binding.clone();
    bad_count.step_leaf_count = 8;
    bad_count.committed_execution_root = manual_execution(
        bad_count.job_context.context_hash(),
        bad_count.full_logits_trace_root,
        bad_count.class.class_id(&bad_count.artifact_root),
        bad_count.step_leaf_count,
        bad_count.step_merkle_root,
    );
    assert!(verify_tir_binding_v2(&bad_count, 8).is_err());
}

#[test]
fn fixed_checkpoints_and_windowed_history_keep_legacy_lanes_beside_wide_final_logits() {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 4], false);
    let scale = p.konst(DType::I64, &[4], &[1 << 32, 1 << 33, 1 << 34, 1 << 35]);
    let fixed = p.fixed_state("last_row", DType::I8, &[4], -128, 127, false);
    let hist = p.hist_state("rows", DType::I8, &[4], 3, false);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0); // slot 0, committed carry
    pre.state_write(fixed, row); // slot 1, original uncommitted state write
    let window = pre.hist_append(hist, row); // slot 2, H=min(a+1,3)
    let window = pre.cast(window, DType::I32); // slot 3, H tiles of four lanes
    pre.commit(window);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let narrow = post.cast(Ref::CarryIn(0), DType::I32); // slot 4
    post.commit(narrow);
    let logits = post.mul(narrow, scale, DType::I64); // slot 5
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let (mut class, artifact, mut job) = fixture(true);
    class.program = program.encode();
    class.layout.commit_tiles = vec![4; 4];
    class.layout.state_tiles = vec![4; 2];
    class.layout.checkpoint_interval = 3;
    class.layout.h_tile = 2;
    job.shape_profile_id = class.class_id(&artifact);
    job.exact_decode_tokens = 6;
    let original = class.clone();
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    assert_eq!(space.program_slots(), 6);
    assert_eq!(space.fixed_instances().len(), 1);
    assert_eq!(space.hist_instances().len(), 1);
    assert_eq!(space.leaf_count_capped(&job, 42).unwrap(), 42);
    assert!(space.leaf_count_capped(&job, 41).is_err());
    // Seven positions: pre row + H window tiles; two post leaves from a=1;
    // one Fixed tile at a=2,5; one two-row Hist tile at a=1,3,5.
    let counts = [2, 6, 7, 7, 6, 8, 6];
    let starts = [0, 2, 8, 15, 22, 28, 36];
    let shape = space.job_shape(&job).unwrap();
    let mut expected = vec![];
    for a in 0..7u32 {
        expected.push((a, 0, 0, DType::I8, 4));
        for tile in 0..(a + 1).min(3) {
            expected.push((a, 3, tile, DType::I32, 4));
        }
        if a >= 1 {
            expected.push((a, 4, 0, DType::I32, 4));
            expected.push((a, 5, 0, DType::I64, 4));
        }
        if (a + 1) % 3 == 0 {
            expected.push((a, 6, 0, DType::I8, 4));
        }
        if (a + 1) % 2 == 0 {
            expected.push((a, 7, 0, DType::I8, 8));
        }
        assert_eq!(space.leaves_at_position(&shape, a), counts[a as usize]);
        assert_eq!(space.running_total(&shape, a), starts[a as usize]);
        let leaves = space.leaves_of_position(&job, a);
        assert_eq!(leaves.len() as u64, counts[a as usize]);
        assert_eq!(leaves[0].index, starts[a as usize] as u64);
    }
    assert_eq!(expected.len(), 42);
    let class_id = class.class_id(&artifact);
    let mut builder = PalwTirStepLegBuilderV2::new(&space, &job, class_id, 42).unwrap();
    let mut hashes = vec![];
    let (mut fixed_count, mut hist_count, mut wide_count) = (0, 0, 0);
    for (index, &(position, slot, tile, dtype, count)) in expected.iter().enumerate() {
        let leaf = space.leaf_at(&job, index as u64).unwrap();
        assert_eq!(
            (leaf.position, leaf.coord.node_slot, leaf.coord.tile_index, leaf.dtype, leaf.value_count),
            (position, slot, tile, dtype, count)
        );
        assert_eq!(leaf.index, index as u64);
        assert_eq!(space.leaf_index(&job, &leaf.coord), Some(index as u64));
        assert_eq!((leaf.coord.call_index, leaf.coord.position), if position < 2 { (0, position) } else { (position - 1, 0) });
        match leaf.kind {
            PalwTirLeafKindV1::State { instance, state, layer, first_element } => {
                fixed_count += 1;
                assert_eq!((slot, instance, state, layer, first_element), (6, 0, fixed, None, 0));
                assert!([2, 5].contains(&position));
            }
            PalwTirLeafKindV1::HistTile { instance, state, layer, first_lane, row_lanes, first_position } => {
                hist_count += 1;
                assert_eq!((slot, instance, state, layer, first_lane, row_lanes), (7, 0, hist, None, 0, 4));
                assert_eq!(first_position, position - 1);
                assert!([1, 3, 5].contains(&position));
            }
            PalwTirLeafKindV1::Commit { first_element, .. } => {
                assert_eq!(first_element, u64::from(tile) * 4);
            }
        }
        // Structural data only; these do not claim to execute the model.
        let lanes: Vec<_> =
            (0..count).map(|i| if dtype == DType::I64 { (i128::from(i) - 2) << 35 } else { i128::from(i) - 4 }).collect();
        let preimage = palw_tir_leaf_preimage_v2(&space, &job, &leaf, &lanes).unwrap();
        if slot == 5 {
            wide_count += 1;
            assert_eq!(preimage.version, 2);
            assert_eq!(preimage.values_le.len(), 32);
            assert_eq!(borsh::to_vec(&preimage).unwrap().len(), 58);
        } else {
            assert_eq!(preimage.version, 1);
            assert_eq!(preimage.values_le.len(), count as usize * 4);
            assert_eq!(preimage, palw_tir_leaf_preimage_v1(&leaf, &lanes).unwrap());
            if slot == 6 || slot == 7 {
                assert_eq!(&preimage.values_le[..4], &[252, 255, 255, 255]);
            }
        }
        // Existing Borsh DTO: u16 version + four u32 coordinates + u32 count
        // + u32 Vec length. This is payload accounting, not a new wire constant.
        assert_eq!(borsh::to_vec(&preimage).unwrap().len(), 26 + preimage.values_le.len());
        assert_eq!(palw_tir_leaf_values_v2(&space, &job, &leaf, &preimage).unwrap(), lanes);
        hashes.push(manual_leaf_hash(job.context_hash(), class_id, &preimage));
        assert_eq!(builder.push(&lanes).unwrap(), leaf);
    }
    assert_eq!((fixed_count, hist_count, wide_count), (2, 3, 6));
    assert!(space.leaf_at(&job, 42).is_none());
    let (step_leaf_count, step_merkle_root) = builder.finish().unwrap();
    assert_eq!(step_leaf_count, 42);
    assert_eq!(step_merkle_root, manual_tree(&hashes));
    let trace = Hash64::from_u64_word(73);
    let binding = PalwTirStepBindingV1 {
        version: 1,
        job_context: job.clone(),
        class: class.clone(),
        artifact_root: artifact,
        full_logits_trace_root: trace,
        step_leaf_count,
        step_merkle_root,
        committed_execution_root: manual_execution(job.context_hash(), trace, class_id, step_leaf_count, step_merkle_root),
    };
    let verified = verify_tir_binding_v2(&binding, 42).unwrap();
    assert_eq!(verified.context_hash(), job.context_hash());
    assert_eq!(verified.class_id(), class_id);
    assert_eq!(class, original);
}
