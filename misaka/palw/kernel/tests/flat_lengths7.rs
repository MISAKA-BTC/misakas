//! Bounded declarative specialization components, not a native media task.
#![allow(warnings)]
use misaka_palw_kernel::{flat_lengths7::*, flat_scope7::FlatRelationCourtV7, job::DecodeRuleV1, trace::trace_v1};
use misaka_palw_tir::{DType, MapParams, Tensor, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};

fn template() -> FlatLengthTemplateV7 {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let token = b.param("source.token", DType::I32, &[8], false);
    let patch = b.param("source.patch", DType::I32, &[6], false);
    let audio = b.param("source.audio", DType::I32, &[4], false);
    let mut pre = b.block("selection", vec![]);
    let token = pre.topk(token, 0, 1);
    let patch = pre.topk(patch, 0, 1);
    let audio = pre.topk(audio, 0, 1);
    let token = pre.cast(token, DType::I32);
    let patch = pre.cast(patch, DType::I32);
    let audio = pre.cast(audio, DType::I32);
    let ids = pre.concat(&[token, patch, audio], 0);
    let pre = pre.finish(&[ids]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[3])]);
    let y = post.cast(misaka_palw_tir::Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    FlatLengthTemplateV7 {
        version: 7,
        maxima: FlatJobLengthsV7 { token: 8, patch: 6, audio: 4 },
        max_program_bytes: p.encode(),
        bindings: [FlatLengthAxisV7::Token, FlatLengthAxisV7::Patch, FlatLengthAxisV7::Audio]
            .into_iter()
            .enumerate()
            .map(|(parameter, length)| FlatShapeBindingV7 {
                slot: FlatShapeSlotV7::Parameter { parameter: parameter as u16, axis: 0 },
                length,
            })
            .collect(),
    }
}

#[test]
fn three_declared_lengths_specialize_one_exact_frozen_program_and_point_scope() {
    let t = template();
    let actual = FlatJobLengthsV7 { token: 3, patch: 2, audio: 1 };
    let s = t.specialize(actual, 3, false).unwrap();
    assert_eq!(s.program.params.iter().map(|p| p.shape[0]).collect::<Vec<_>>(), vec![3, 2, 1]);
    assert_eq!(s.scope.max_positions, 3);
    assert_eq!(s.scope.relations.iter().filter(|r| r.court == FlatRelationCourtV7::TopKPointV7).count(), 3);
    assert_eq!(s.admission.scope_root, s.scope.root());
    assert_eq!(s.admission.replay_root, s.replay.root());
    assert_eq!(t.check_admission(&s.admission, false).unwrap().program, s.program);
    assert_eq!(FlatLengthTemplateV7::from_bytes(&t.to_bytes().unwrap()).unwrap(), t);
    assert_ne!(s.admission.root(), t.specialize(t.maxima, 3, false).unwrap().admission.root());
    let params = MapParams {
        tensors: [
            ((0, None), Tensor::new(DType::I32, vec![3], vec![7, 7, 6]).unwrap()),
            ((1, None), Tensor::new(DType::I32, vec![2], vec![0, 2]).unwrap()),
            ((2, None), Tensor::new(DType::I32, vec![1], vec![9]).unwrap()),
        ]
        .into(),
    };
    let trace = trace_v1(&s.program, &params, &[0, 0, 0]).unwrap();
    let logits = trace.values[2].last().unwrap().last().unwrap();
    assert_eq!(logits.data, vec![0, 1, 0], "tie takes the lower token index and each axis has its admitted extent");
    assert_eq!(DecodeRuleV1::Greedy.select(logits), Some(1));
    let borrowed = MapParams {
        tensors: [
            ((0, None), Tensor::new(DType::I32, vec![8], vec![0; 8]).unwrap()),
            ((1, None), Tensor::new(DType::I32, vec![2], vec![0, 2]).unwrap()),
            ((2, None), Tensor::new(DType::I32, vec![1], vec![9]).unwrap()),
        ]
        .into(),
    };
    assert!(trace_v1(&s.program, &borrowed, &[0]).is_err(), "maximum-shape parameters cannot substitute for actual-shape parameters");
}

#[test]
fn exact_admission_refuses_every_rebound_or_enlarged_field() {
    let t = template();
    let s = t.specialize(FlatJobLengthsV7 { token: 3, patch: 2, audio: 1 }, 3, false).unwrap();
    for field in 0..8 {
        let mut changed = s.admission.clone();
        match field {
            0 => changed.version += 1,
            1 => changed.template_root[0] ^= 1,
            2 => changed.actual.token += 1,
            3 => changed.actual.patch += 1,
            4 => changed.program_root[0] ^= 1,
            5 => changed.positions += 1,
            6 => changed.scope_root[0] ^= 1,
            _ => changed.replay_root[0] ^= 1,
        }
        assert!(t.check_admission(&changed, false).is_err(), "changed field {field}");
    }
    for lengths in [
        FlatJobLengthsV7 { token: 9, patch: 2, audio: 1 },
        FlatJobLengthsV7 { token: 3, patch: 7, audio: 1 },
        FlatJobLengthsV7 { token: 3, patch: 2, audio: 5 },
        FlatJobLengthsV7 { token: 3, patch: 0, audio: 1 },
    ] {
        assert!(t.specialize(lengths, 3, false).is_err());
    }
    assert!(s.admission.check_stage_counts(FlatJobLengthsV7 { token: 3, patch: 2, audio: 2 }).is_err());
    s.admission.check_stage_counts(s.admission.actual).unwrap();
}

#[test]
fn map_order_duplicate_absent_axis_and_type_mismatch_are_refused() {
    let t = template();
    let actual = FlatJobLengthsV7 { token: 3, patch: 2, audio: 1 };
    let mut bad = t.clone();
    bad.bindings.swap(0, 1);
    assert!(bad.specialize(actual, 3, false).is_err());
    let mut bad = t.clone();
    bad.bindings[1].slot = bad.bindings[0].slot;
    assert!(bad.specialize(actual, 3, false).is_err());
    let mut bad = t.clone();
    bad.bindings[0].slot = FlatShapeSlotV7::Parameter { parameter: u16::MAX, axis: 0 };
    assert!(bad.specialize(actual, 3, false).is_err());
    let mut bad = t.clone();
    bad.maxima.audio = 0;
    assert!(bad.specialize(FlatJobLengthsV7 { audio: 0, ..actual }, 3, false).is_err());
    let mut bad = t.clone();
    bad.bindings[0].length = FlatLengthAxisV7::Patch;
    assert!(bad.specialize(actual, 3, false).is_err(), "slot must match its own declared maximum");
    let mut bad = t.clone();
    bad.maxima.token = u32::MAX;
    assert!(bad.specialize(actual, 3, false).is_err());
}

#[test]
fn canonical_counts_are_bounded_before_variable_allocation() {
    let t = template();
    let bytes = t.to_bytes().unwrap();
    for n in 0..bytes.len() {
        assert!(FlatLengthTemplateV7::from_bytes(&bytes[..n]).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(FlatLengthTemplateV7::from_bytes(&extra).is_err());
    let mut bad = bytes.clone();
    bad[..2].copy_from_slice(&1u16.to_le_bytes());
    assert!(FlatLengthTemplateV7::from_bytes(&bad).is_err(), "v1 cannot select the new template contract");
    let mut bad = bytes.clone();
    bad[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatLengthTemplateV7::from_bytes(&bad).is_err());
    let at = 18 + t.max_program_bytes.len();
    let mut bad = bytes;
    bad[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatLengthTemplateV7::from_bytes(&bad).is_err());
    assert!(FlatLengthTemplateV7::from_bytes(&vec![0; FLAT_LENGTH_MAX_WIRE_BYTES_V7 + 1]).is_err());
}

#[test]
fn text_job_binding_freezes_all_fed_back_tokens_and_refuses_unbound_modalities() {
    let t = template();
    let s = t.specialize(FlatJobLengthsV7 { token: 3, patch: 2, audio: 1 }, 3, false).unwrap();
    assert!(s.admission.check_decoder_job(2, 2, 3).is_err(), "media counts cannot enter the text-only native route");
    let mut a = s.admission.clone();
    a.actual.patch = 0;
    a.actual.audio = 0;
    a.check_decoder_job(2, 2, 3).unwrap();
    assert!(a.check_decoder_job(2, 3, 3).is_err());
    assert!(a.check_decoder_job(1, 2, 3).is_err());
    assert!(a.check_decoder_job(2, 2, 4).is_err());
    assert!(a.check_decoder_job(0, 4, 3).is_err());
    assert!(a.check_decoder_job(3, 0, 3).is_err());
    a.positions = 1;
    a.check_encoder_job(3, 1).unwrap();
    assert!(a.check_encoder_job(4, 1).is_err());
    assert!(a.check_encoder_job(3, 2).is_err());
}
