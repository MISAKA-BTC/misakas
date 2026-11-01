use super::*;
use crate::{
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_admission_v1::{PalwTirClassRecordV1, palw_tir_class_record_v1},
    palw_tir_class_record_v2::{ProfiledTirClassRecordV2, TirClassRecordProfileLimitsV2},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_profile_resolver_v2::TirTraceProfileV2,
};
use misaka_palw_tir::{
    DType, Dim, Node, Prim, Ref, TensorType,
    builder::ProgramBuilder,
    program::{HISTORY_BOUND_V1_HELD, HISTORY_BOUND_V1_SMALL, MAX_PROGRAM_BYTES},
};
use std::sync::Arc;

fn limits() -> TirRowProfileLimitsV2 {
    TirRowProfileLimitsV2 { max_program_bytes: MAX_PROGRAM_BYTES, max_dissected_points: 8192 }
}
fn build(class: &PalwTirClassV1, artifact: Hash64) -> PalwTirClassRecordV1 {
    ProfiledTirClassRecordV2::build(
        class,
        artifact,
        TirClassRecordProfileLimitsV2 { max_program_bytes: MAX_PROGRAM_BYTES, max_layout_entries: 8192 },
    )
    .unwrap()
    .into_storage_row()
}
fn fixture(profile: TirTraceProfileV2, held: bool, vocab: u32) -> (PalwTirClassV1, Hash64) {
    let mut p = ProgramBuilder::new(4, if held { HISTORY_BOUND_V1_HELD } else { HISTORY_BOUND_V1_SMALL });
    let embed = p.param("embed", DType::I8, &[4, vocab], false);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[vocab])]);
    let narrow = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(narrow);
    let logits = if profile == TirTraceProfileV2::TiledI64Q24 {
        let out = post.cast(narrow, DType::I64);
        post.commit(out);
        out
    } else {
        narrow
    };
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = profile.program_scheme_id().as_bytes();
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(18),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: vec![4; if profile == TirTraceProfileV2::TiledI64Q24 { 3 } else { 2 }],
            state_tiles: vec![],
        },
    };
    (class, Hash64::from_u64_word(29))
}
fn resolve(row: &PalwTirClassRecordV1, artifact: Hash64) -> ResolvedTirRowProfileV2 {
    ResolvedTirRowProfileV2::resolve(row.facts.class_id, artifact, row, limits()).unwrap()
}
fn row_profile(profile: TirTraceProfileV2) -> TirRowTraceProfileV2 {
    match profile {
        TirTraceProfileV2::FlatI32 => TirRowTraceProfileV2::FlatLegacy4Byte,
        TirTraceProfileV2::TiledI32 => TirRowTraceProfileV2::TiledLegacy4Byte,
        TirTraceProfileV2::TiledI64Q24 => TirRowTraceProfileV2::TiledI64Q24,
    }
}

#[test]
fn narrow_row_bytes_identity_and_profile_metadata_stay_unchanged() {
    for profile in [TirTraceProfileV2::FlatI32, TirTraceProfileV2::TiledI32] {
        for held in [false, true] {
            let (class, artifact) = fixture(profile, held, 8);
            let (legacy, _) = palw_tir_class_record_v1(&class, &artifact).unwrap();
            let row = build(&class, artifact);
            assert_eq!(row, legacy);
            assert!(reject_known_wide_tir_row_v1(&row).is_ok());
            let bytes = borsh::to_vec(&row).unwrap();
            let rooted = row.rooted_bytes_v1();
            let view = resolve(&row, artifact);
            assert_eq!(view.class_id(), class.class_id(&artifact));
            assert_eq!(view.profile(), row_profile(profile));
            assert_eq!(view.logits_dtype(), DType::I32);
            assert_eq!(view.program_scheme_id(), profile.program_scheme_id());
            let trace =
                if profile == TirTraceProfileV2::FlatI32 { crate::palw_v2::trace_scheme_id_v2() } else { profile.program_scheme_id() };
            assert_eq!(view.context_trace_scheme_id(), trace);
            assert_eq!((view.token_bound(), view.held(), view.row_max_context()), (4, held, 64));
            assert_eq!((view.logits_vocab(), view.lane_width_bytes(), view.logits_tiles()), (8, 4, 1));
            for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
                assert_eq!(view.prompt_ids_form(form), if held { PalwPromptIdsFormV1::MerkleV1 } else { form });
            }
            assert_eq!(borsh::to_vec(&row).unwrap(), bytes);
            assert_eq!(row.rooted_bytes_v1(), rooted);
            assert_eq!(row.program.as_ref(), &class.program);
        }
    }
}

#[test]
fn wide_4097_vocab_dispatches_two_tiles_without_redefining_old_tiled_bit() {
    let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 4097);
    let row = build(&class, artifact);
    assert!(!row.facts.tiled);
    assert_eq!(row.logits_tiles(), 1);
    let raw = borsh::to_vec(&row).unwrap();
    let stored: PalwTirClassRecordV1 = borsh::from_slice(&raw).unwrap();
    let view = resolve(&stored, artifact);
    assert_eq!(view.profile(), TirRowTraceProfileV2::TiledI64Q24);
    assert_eq!(view.logits_dtype(), DType::I64);
    assert_eq!((view.logits_vocab(), view.lane_width_bytes(), view.logits_tiles()), (4097, 8, 2));
    assert_eq!(view.context_trace_scheme_id(), crate::palw_logits_q24_v2::scheme_id());
    assert_eq!(view.program_scheme_id(), crate::palw_logits_q24_v2::scheme_id());
    assert_eq!(view.class_id(), class.class_id(&artifact));
    assert_eq!(borsh::to_vec(&stored).unwrap(), raw);
    assert!(palw_tir_class_record_v1(&class, &artifact).is_err());
}

#[test]
fn legacy_reader_guard_denies_wide_4097_without_reinterpreting_the_old_fact_bit() {
    let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 4097);
    let mut row = build(&class, artifact);
    let bytes = borsh::to_vec(&row).unwrap();
    assert!(!row.facts.tiled);
    assert_eq!(row.logits_tiles(), 1);
    assert!(reject_known_wide_tir_row_v1(&row).is_err());
    assert!(crate::palw_tir_fp_lane_v1::palw_tir_fp_class_v1(&row, &class.layout).unwrap_err().contains("wide Q24"));
    assert!(crate::palw_tir_fp_lane_v1::palw_tir_fp_lane_covered_v1(&row, std::iter::empty()).unwrap_err().contains("wide Q24"));
    assert_eq!(borsh::to_vec(&row).unwrap(), bytes);
    assert_eq!(resolve(&row, artifact).logits_tiles(), 2);
    // A raw facts hint cannot bypass the guard on the original program bytes.
    row.facts.tiled = true;
    assert!(reject_known_wide_tir_row_v1(&row).is_err());
}

#[test]
fn legacy_reader_guard_does_not_authenticate_empty_unknown_or_synthetic_rows() {
    let (class, artifact) = fixture(TirTraceProfileV2::TiledI32, false, 8);
    let mut row = build(&class, artifact);
    for bytes in [Vec::new(), vec![0; 63], vec![0x88; 64], vec![0xff; 128]] {
        row.program = Arc::new(bytes);
        // The existing consumer still has to validate these bytes. This guard
        // promises only that the known wide footer cannot reach its fallback.
        assert!(reject_known_wide_tir_row_v1(&row).is_ok());
        assert!(row.check_program_v1().is_err());
    }
}

#[test]
fn keys_artifacts_digests_and_every_program_derived_row_field_are_checked() {
    let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 8);
    let original = build(&class, artifact);
    let id = original.facts.class_id;
    let foreign = Hash64::from_u64_word(91);
    assert!(ResolvedTirRowProfileV2::resolve(foreign, artifact, &original, limits()).is_err());
    assert!(ResolvedTirRowProfileV2::resolve(id, foreign, &original, limits()).is_err());
    for mutation in 0..12 {
        let mut row = original.clone();
        match mutation {
            0 => row.facts.class_id = foreign,
            1 => row.graph_ir_root = foreign,
            2 => row.layout_digest = foreign,
            3 => row.tokenizer_id = foreign,
            4 => row.facts.token_bound += 1,
            5 => row.facts.held = true,
            6 => row.facts.tiled = true,
            7 => row.prim_set_id = foreign,
            8 => row.logits_vocab += 1,
            9 => row.program_bytes += 1,
            10 => row.dissected.push((0, 0)),
            _ => {
                let mut bytes = row.program.as_ref().clone();
                bytes[0] ^= 1;
                row.program = Arc::new(bytes);
            }
        }
        assert!(ResolvedTirRowProfileV2::resolve(id, artifact, &row, limits()).is_err(), "accepted row mutation {mutation}");
    }
    let mut row = original;
    row.version = 2;
    assert!(matches!(ResolvedTirRowProfileV2::resolve(id, artifact, &row, limits()), Err(TirRowProfileErrorV2::RowVersion)));
}

#[test]
fn in_range_context_is_explicit_row_metadata_and_out_of_range_is_refused() {
    let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 8);
    let mut row = build(&class, artifact);
    let id = row.facts.class_id;
    let history = class.decode_program().unwrap().history_bound;
    // No layout preimage is supplied: in-range row metadata is exposed, not
    // represented as independently reconstructed geometry or a native permit.
    for context in [1, 63, 65, history] {
        row.facts.max_context = context;
        assert_eq!(resolve(&row, artifact).row_max_context(), context);
    }
    for context in [0, history + 1] {
        row.facts.max_context = context;
        assert!(matches!(ResolvedTirRowProfileV2::resolve(id, artifact, &row, limits()), Err(TirRowProfileErrorV2::ContextBound)));
    }
}

#[test]
fn raw_caps_precede_hashing_or_decode_and_unknown_scheme_never_falls_back() {
    let (class, artifact) = fixture(TirTraceProfileV2::TiledI32, false, 8);
    let row = build(&class, artifact);
    let id = row.facts.class_id;
    let mut invalid = row.clone();
    invalid.version = 99;
    invalid.program = Arc::new(vec![0xff; row.program.len()]);
    for max_program_bytes in [0, row.program.len() - 1] {
        let cap = TirRowProfileLimitsV2 { max_program_bytes, ..limits() };
        assert!(matches!(ResolvedTirRowProfileV2::resolve(id, artifact, &invalid, cap), Err(TirRowProfileErrorV2::Budget)));
    }
    // Zero dissection capacity is sufficient when the graph has no such sites.
    let zero = TirRowProfileLimitsV2 { max_dissected_points: 0, ..limits() };
    assert!(ResolvedTirRowProfileV2::resolve(id, artifact, &row, zero).is_ok());
    let mut over = row.clone();
    over.dissected.push((0, 0));
    assert!(matches!(ResolvedTirRowProfileV2::resolve(id, artifact, &over, zero), Err(TirRowProfileErrorV2::Budget)));
    let mut unknown = class;
    let mut program = unknown.decode_program().unwrap();
    program.logits_scheme_id = [0x88; 64];
    unknown.program = program.encode();
    let mut unrecognized = row;
    unrecognized.program = Arc::new(unknown.program.clone());
    unrecognized.program_bytes = unknown.program.len() as u32;
    unrecognized.graph_ir_root = unknown.graph_ir_root();
    unrecognized.facts.class_id = unknown.class_id(&artifact);
    // The registry key, graph hash and full class identity all agree, so a
    // structural validator or the scheme dispatcher must refuse this profile.
    assert!(matches!(
        ResolvedTirRowProfileV2::resolve(unrecognized.facts.class_id, artifact, &unrecognized, limits()),
        Err(TirRowProfileErrorV2::UnsupportedScheme | TirRowProfileErrorV2::Program(_))
    ));
}

#[test]
fn nonempty_dissection_metadata_is_compared_canonically_under_its_own_cap() {
    let mut p = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let history = p.hist_state("rows", DType::I32, &[1], 3, true);
    let carry = TensorType::fixed(DType::I32, &[1]);
    let mut pre = p.block("pre", vec![]);
    let x = pre.iota(DType::I32, &[Dim::Fixed(1)], 0, 1, 0);
    let pre = pre.finish(&[x]);
    let mut layer = p.block("layer", vec![carry.clone()]);
    let rows = layer.hist_append(history, Ref::CarryIn(0));
    let sum = layer.reduce_sum(rows, 0, DType::I64);
    let sum = layer.reshape_fixed(sum, &[1]);
    let narrowed = layer.clamp(sum, -1000, 1000, DType::I32);
    let Ref::Node(site) = narrowed else { unreachable!() };
    let layer = layer.finish(&[narrowed]);
    let mut post = p.block("post", vec![carry]);
    let output = post.cast(Ref::CarryIn(0), DType::I64);
    post.commit(output);
    let Ref::Node(logits) = output else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![layer], post, logits);
    program.logits_scheme_id = TirTraceProfileV2::TiledI64Q24.program_scheme_id().as_bytes();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(19),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 1,
            commit_tiles: vec![4; 3],
            state_tiles: vec![4],
        },
    };
    let artifact = Hash64::from_u64_word(31);
    let row = build(&class, artifact);
    assert_eq!(row.dissected, vec![(layer, site)]);
    assert_eq!(resolve(&row, artifact).profile(), TirRowTraceProfileV2::TiledI64Q24);
    let cap = TirRowProfileLimitsV2 { max_dissected_points: 0, ..limits() };
    assert!(matches!(ResolvedTirRowProfileV2::resolve(row.facts.class_id, artifact, &row, cap), Err(TirRowProfileErrorV2::Budget)));
    let mut omitted = row.clone();
    omitted.dissected.clear();
    assert!(ResolvedTirRowProfileV2::resolve(row.facts.class_id, artifact, &omitted, limits()).is_err());
    let mut substituted = row.clone();
    substituted.dissected[0].1 += 1;
    assert!(ResolvedTirRowProfileV2::resolve(row.facts.class_id, artifact, &substituted, limits()).is_err());
    let mut duplicate = row.clone();
    duplicate.dissected.push((layer, site));
    assert!(ResolvedTirRowProfileV2::resolve(row.facts.class_id, artifact, &duplicate, limits()).is_err());
}

#[test]
fn legacy_i8_i16_and_idx_outputs_keep_old_factory_bytes_and_four_byte_wire_lanes() {
    for profile in [TirTraceProfileV2::FlatI32, TirTraceProfileV2::TiledI32] {
        for dtype in [DType::I8, DType::I16, DType::Idx] {
            let (mut class, artifact) = fixture(profile, false, 8);
            let mut program = class.decode_program().unwrap();
            let post = &mut program.blocks[program.schedule.post as usize];
            let mut input = program.logits;
            if dtype == DType::Idx {
                // Construct a valid nonnegative index interval; do not rely on
                // truncation or an unchecked signed-to-index conversion.
                post.nodes.push(Node {
                    prim: Prim::Clamp { lo: 0, hi: 127 },
                    inputs: vec![Ref::Node(input)],
                    out: TensorType::fixed(DType::I32, &[8]),
                    commit: false,
                });
                input = (post.nodes.len() - 1) as u16;
            }
            post.nodes.push(Node {
                prim: Prim::Cast,
                inputs: vec![Ref::Node(input)],
                out: TensorType::fixed(dtype, &[8]),
                commit: true,
            });
            program.logits = (post.nodes.len() - 1) as u16;
            misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
            class.program = program.encode();
            class.layout.commit_tiles.push(4);
            // The old factory accepts all old committable output dtypes. This
            // metadata-only view preserves that broader historical behavior.
            let (legacy, _) = palw_tir_class_record_v1(&class, &artifact).unwrap();
            let bytes = borsh::to_vec(&legacy).unwrap();
            let rooted = legacy.rooted_bytes_v1();
            let view = resolve(&legacy, artifact);
            assert!(reject_known_wide_tir_row_v1(&legacy).is_ok());
            assert_eq!(view.class_id(), class.class_id(&artifact));
            assert_eq!(view.profile(), row_profile(profile));
            assert_eq!(view.logits_dtype(), dtype);
            assert_eq!(view.lane_width_bytes(), 4);
            assert_eq!((view.logits_vocab(), view.logits_tiles()), (8, 1));
            assert_eq!(borsh::to_vec(&legacy).unwrap(), bytes);
            assert_eq!(legacy.rooted_bytes_v1(), rooted);
            assert_eq!(legacy.program.as_ref(), &class.program);
        }
    }
}

#[test]
fn fixed_program_format_ceiling_bounds_actual_and_declared_bytes_independently() {
    let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 8);
    let row = build(&class, artifact);
    let id = row.facts.class_id;
    let exact = TirRowProfileLimitsV2 { max_program_bytes: row.program.len(), max_dissected_points: row.dissected.len() };
    assert!(ResolvedTirRowProfileV2::resolve(id, artifact, &row, exact).is_ok());
    let generous = TirRowProfileLimitsV2 { max_program_bytes: MAX_PROGRAM_BYTES + 1, ..limits() };
    let mut actual_over = row.clone();
    actual_over.program = Arc::new(vec![0xff; MAX_PROGRAM_BYTES + 1]);
    assert!(actual_over.program_bytes as usize <= MAX_PROGRAM_BYTES);
    actual_over.version = 99;
    assert!(matches!(ResolvedTirRowProfileV2::resolve(id, artifact, &actual_over, generous), Err(TirRowProfileErrorV2::Budget)));
    let mut declared_over = row;
    declared_over.program_bytes = (MAX_PROGRAM_BYTES + 1) as u32;
    assert!(declared_over.program.len() <= MAX_PROGRAM_BYTES);
    declared_over.version = 99;
    assert!(matches!(ResolvedTirRowProfileV2::resolve(id, artifact, &declared_over, generous), Err(TirRowProfileErrorV2::Budget)));
}

#[test]
fn rehashed_noncanonical_and_invalid_wide_programs_reach_structural_parser() {
    let (class, artifact) = fixture(TirTraceProfileV2::TiledI64Q24, false, 8);
    let row = build(&class, artifact);
    let assert_program_error = |bytes: Vec<u8>| {
        let mut changed_class = class.clone();
        changed_class.program = bytes;
        let mut changed_row = row.clone();
        changed_row.program = Arc::new(changed_class.program.clone());
        changed_row.program_bytes = changed_class.program.len() as u32;
        changed_row.graph_ir_root = changed_class.graph_ir_root();
        changed_row.facts.class_id = changed_class.class_id(&artifact);
        // The canonical byte digest and independently supplied key agree with
        // this altered payload. Rejection must come from parsing/validation.
        changed_row.check_program_v1().unwrap();
        assert!(matches!(
            ResolvedTirRowProfileV2::resolve(changed_row.facts.class_id, artifact, &changed_row, limits()),
            Err(TirRowProfileErrorV2::Program(_))
        ));
    };
    let mut trailing = class.program.clone();
    trailing.push(0);
    assert_program_error(trailing);
    let mut truncated = class.program.clone();
    truncated.pop();
    assert_program_error(truncated);
    for dtype in [DType::I128, DType::I32] {
        let mut invalid = class.decode_program().unwrap();
        invalid.blocks[invalid.schedule.post as usize].nodes[invalid.logits as usize].out.dtype = dtype;
        assert_program_error(invalid.encode());
    }
}
