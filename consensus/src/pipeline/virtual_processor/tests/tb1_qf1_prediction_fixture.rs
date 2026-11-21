//! A small stateful PALWTIR1 for checking the QF1 schedule and observer.
//! Its arithmetic and logits have an independent closed-form oracle. These
//! test tensors are unrelated to Qwen weights, RMSNorm, or mining approval.
use super::QF1_TEST_TOKENIZER;
use kaspa_consensus_core::{
    Hash64,
    palw_qf1::{self, ROLES, Readout, Stop, VOCAB, WIDTH},
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1},
};
use misaka_palw_tir::{DType, Ref, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};
use misaka_palw_tir_exec::node::{
    TirArtifactV1,
    qf1::{LocalAnswer, LocalInstance},
};
use std::path::Path;

fn increment(t: u32) -> i16 {
    if t == 7 { -10000 } else { (1 + t % 7) as i16 }
}

pub(super) fn artifact(path: &Path) -> (TirArtifactV1, u16) {
    let mut p = ProgramBuilder::new(VOCAB, HISTORY_BOUND_V1_SMALL);
    let table = p.param("token_increment", DType::I16, &[VOCAB], false);
    let signs = p.param("logit_sign", DType::I8, &[VOCAB], false);
    let lanes = p.konst(DType::I8, &[WIDTH as u32], &(0..WIDTH).map(|i| (i % 7) as i128 - 3).collect::<Vec<_>>());
    let acc = p.fixed_state("prefix_sum", DType::I16, &[], -32767, 32767, false);
    let one = p.scalar(DType::I16, 1);
    let five = p.scalar(DType::I16, 5);
    let mut pre = p.block("prefill", vec![]);
    let delta = pre.gather(table, Ref::Input(0), 0, 0);
    let sum = pre.add(Ref::State(acc), delta, DType::I32);
    let state = pre.state_write(acc, sum);
    let x = pre.add(state, lanes, DType::I32);
    let x = pre.clamp(x, -32767, 32767, DType::I16);
    let carry = vec![pre.ty(x)];
    let pre_id = pre.finish(&[x]);
    let mut body = p.block("layer", carry.clone());
    let s = body.add(Ref::CarryIn(0), one, DType::I32);
    let residual = body.clamp(s, -32767, 32767, DType::I16);
    let layer = body.finish(&[residual]);
    let mut post = p.block("prediction", carry);
    let s = post.add(Ref::CarryIn(0), five, DType::I32);
    let n = post.clamp(s, -32767, 32767, DType::I16);
    let Ref::Node(norm) = post.commit(n) else { unreachable!() };
    let sign = post.reduce_max(n, 0);
    let scores = post.mul(sign, signs, DType::I32);
    let Ref::Node(head) = post.commit(scores) else { unreachable!() };
    post.topk(scores, 0, 1);
    let post_id = post.finish(&[]);
    let mut prog = p.finish(pre_id, vec![layer; ROLES - 1], post_id, head);
    prog.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
    let layout = PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: palw_qf1::CONTEXT,
        checkpoint_interval: 2,
        h_tile: 2,
        commit_tiles: prog.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).map(|_| 64).collect(),
        state_tiles: vec![4],
    };
    let tokenizer: Hash64 = QF1_TEST_TOKENIZER.parse().unwrap();
    misaka_palw_tir_artifact::write_container_v1(
        path,
        &prog,
        borsh::to_vec(&layout).unwrap(),
        tokenizer.as_bytes(),
        "stateful synthetic schedule fixture".into(),
        &mut |param, _| {
            Ok(match param {
                0 => (0..VOCAB).flat_map(|id| increment(id).to_le_bytes()).collect(),
                1 => (0..VOCAB)
                    .map(|id| match id {
                        7 | 11 => 1,
                        151643 | 151645 => 255,
                        _ => 0,
                    })
                    .collect(),
                _ => unreachable!(),
            })
        },
    )
    .unwrap();
    (TirArtifactV1::open(path).unwrap(), norm)
}

pub(super) fn bind(a: &TirArtifactV1, norm: u16) -> LocalInstance<'_> {
    bind_scale(a, norm, b"unit scale")
}

pub(super) fn bind_scale<'a>(a: &'a TirArtifactV1, norm: u16, units: &[u8]) -> LocalInstance<'a> {
    let manifest = kaspa_hashes::blake2b_512_keyed(
        b"qf1-schedule-test",
        b"prefix-sum with lane offset (i%7)-3 / layer: +1 / final: +5 / signed logit table",
    );
    let scale = kaspa_hashes::blake2b_512_keyed(b"qf1-schedule-test", units);
    LocalInstance::bind(a, manifest, norm, &[scale; ROLES]).unwrap()
}

pub(super) fn check(prompt: &[u32], limit: u32, actual: &LocalAnswer) {
    let mut state = 0i32;
    let update = |a: i32, id| (a + i32::from(increment(id))).clamp(-32767, 32767);
    for &id in prompt {
        state = update(state, id);
    }
    let mut answer = Vec::new();
    let mut readout = Readout::default();
    let stop = loop {
        let rows: [Vec<i16>; ROLES] = std::array::from_fn(|role| {
            let k = if role == ROLES - 1 { 41 } else { role as i32 + 1 };
            (0..WIDTH)
                .map(|i| {
                    let input = (state + (i % 7) as i32 - 3).clamp(-32767, 32767);
                    (input + k).clamp(-32767, 32767) as i16
                })
                .collect()
        });
        readout.step(&std::array::from_fn(|r| rows[r].as_slice())).unwrap();
        let id = match rows[ROLES - 1].iter().copied().max().unwrap().cmp(&0) {
            std::cmp::Ordering::Less => 151643,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 7,
        };
        answer.push(id);
        if id == 151643 {
            break Stop::Eos;
        }
        if answer.len() == limit as usize {
            break Stop::Limit;
        }
        state = update(state, id);
    };
    assert_eq!(actual.tokens, answer);
    assert_eq!(actual.stop, stop);
    assert_eq!(actual.processed_positions as usize, prompt.len() + answer.len() - 1);
    assert_eq!(actual.z, readout.finish(prompt.len() as u32, &answer, limit).unwrap().0);
}

#[test]
fn qf1_stateful_prediction_boundaries() {
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let (a, norm) = artifact(&dir.path().join("fixture.palwtir"));
    let instance = bind(&a, norm);
    for (prompt, limit) in [(&[9u32][..], 1), (&[7][..], 3), (&[9, 2][..], 2)] {
        let run = instance.replay(prompt, limit, 400000, Hash64::from_u64_word(1), [1; 32]).unwrap();
        check(prompt, limit, &run);
    }
    assert!(instance.replay(&[9], 1, 1, Hash64::from_u64_word(1), [1; 32]).is_err());
    let h = Hash64::from_u64_word(7);
    assert!(LocalInstance::bind(&a, h, norm + 1, &[h; ROLES]).is_err());
    let path = dir.path().join("fixture.palwtir");
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_len(file.metadata().unwrap().len() + 1).unwrap();
    assert!(instance.replay(&[9], 1, 400000, h, [1; 32]).is_err());
    assert!(LocalInstance::bind(&a, h, norm, &[h; ROLES]).is_err());
}

#[test]
fn qf1_original_requires_actual_stop_and_complete_trace_budget() {
    use misaka_palw_tir_exec::node::qf1::TraceRequest;
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let (a, norm) = artifact(&dir.path().join("budget.palwtir"));
    let instance = bind(&a, norm);
    let id = Hash64::from_u64_word(1);
    let run =
        |t, cap, open| instance.replay_trace(&[9], 3, cap, id, [2; 32], TraceRequest { context_digest: id, answer_len: t, open });
    for t in [0, 1, 3, 4] {
        assert!(run(t, 400000, None).is_err());
    }
    assert!(run(2, 100, None).is_err());
    assert!(run(2, 400000, Some(u64::MAX)).is_err());
    let trace = run(2, 400000, Some(0)).unwrap();
    let o = trace.opening.unwrap();
    assert!(trace.roots[3].verify_opening(0, &o.record, &o.siblings));
    assert_eq!(o.record[0], 0);
    assert_eq!(&o.record[5..69], id.as_byte_slice());
}

/// Reopen the packed full model through the node. The archived independent reference used
/// the same 111 tokens before the registration range and court commitment changes.
#[test]
#[ignore = "requires the verified private 8B PALWTIR1 artifact"]
fn private_qf1_8b_archived_readout() {
    let path = std::env::var_os("MISAKA_PRIVATE_QF1_FULL").expect("full QF1 PALWTIR1 path");
    let a = TirArtifactV1::open(Path::new(&path)).unwrap();
    let local = misaka_palw_sdk::qf1_local::bind(&a).unwrap();
    let prompt: Vec<u32> = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/evidence/palw/mas1-qf1-native-2026-11-21/full-replay-input.json"
    )))
    .unwrap();
    let reference: serde_json::Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/evidence/palw/mas1-qf1-native-2026-11-21/full-replay-check.json"
    )))
    .unwrap();
    let answer = local.replay(&prompt, 3, 32_000_000, Hash64::from_u64_word(1), [0; 32]).unwrap();
    assert_eq!(serde_json::json!(answer.tokens), reference["output_ids"]);
    assert_eq!(serde_json::json!(answer.processed_positions), reference["positions"]);
    assert_eq!(format!("{:?}", answer.stop), reference["stop"].as_str().unwrap());
    let z = Hash64::from_bytes(answer.z);
    assert_eq!(z.to_string(), reference["readout_z"].as_str().unwrap());
    eprintln!(
        "QF1 held replay class={} inventory={} tokens={:?} stop={:?} positions={} z={}",
        local.identity().class,
        local.identity().inventory,
        answer.tokens,
        answer.stop,
        answer.processed_positions,
        z
    );
}
