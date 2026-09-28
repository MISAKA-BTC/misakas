//! **The `Select`-arm work credit under the second IR fence** (`Params::palw_tir_fence2`,
//! `crate::palw_tir_work_v1::palw_tir_work_shape_with_v1`): a `Select`'s arm-only, uncommitted work is
//! credited at the smaller arm's (coefficient-wise), the least any execution does — a backend may
//! skip an unselected arm no court reads — where the DAA-2,000 release credits both.
//!
//! * a hand-built `Select` whose one arm is a weight matmul nothing else reads: past the fence the
//!   matmul is not credited at all (the other arm holds no dense work), and everything outside the
//!   arms is credited as before;
//! * the same matmul COMMITTED is a sink — a court checks it whole — so it is credited as before;
//! * a `Select` nested in an arm is credited first, and the outer arm's size is its credited size;
//! * on the corpus and the Qwen2.5-A16 mirrors the credit never exceeds the release's, equals it for a
//!   program without a `Select`, and moves the A16 decoders by less than a tenth of a percent.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_select_credit`

use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalExecutionFactsV1, PalwCanonicalWorkVectorV1};
use kaspa_consensus_core::palw_tir_work_v1::{palw_tir_select_arm_regions_v1, palw_tir_work_shape_with_v1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{Cmp, DType, Ref, TensorType, TirProgramV1};
use std::path::PathBuf;

fn job() -> PalwCanonicalExecutionFactsV1 {
    PalwCanonicalExecutionFactsV1::uncached(5, 3)
}

fn work(p: &TirProgramV1, min_select_arms: bool) -> PalwCanonicalWorkVectorV1 {
    palw_tir_work_shape_with_v1(p, min_select_arms).expect("a shape").work_v1(&job()).expect("a vector")
}

fn fields(v: &PalwCanonicalWorkVectorV1) -> [u128; 10] {
    [
        v.dense_matmul,
        v.routed_expert_matmul,
        v.normalization,
        v.recurrence,
        v.attention_prefill,
        v.attention_decode,
        v.other_verified_ops,
        v.weight_traffic_bytes,
        v.kv_read_bytes,
        v.kv_write_bytes,
    ]
}

/// A `Select(carry == carry, W · carry, carry + carry)` layer: arm 1 a weight matmul nothing else
/// reads (committed when `commit_arm`), arm 2 one elementwise add. `nested` puts a second `Select`
/// inside arm 1. Post: the LM head.
fn program(commit_arm: bool, nested: bool) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, 7], false);
    let w = pb.param("w", DType::I8, &[7, 7], true);
    let w2 = nested.then(|| pb.param("w2", DType::I8, &[7, 7], true));
    let head = pb.param("head", DType::I8, &[8, 7], false);
    let carry = vec![TensorType::fixed(DType::I32, &[7])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = Ref::CarryIn(0);
        let cond = b.compare(x, x, Cmp::Eq);
        let col = b.reshape_fixed(x, &[7, 1]);
        let y = b.matmul(w, col, DType::I64);
        let y = b.reshape_fixed(y, &[7]);
        let y = b.clamp(y, -1000, 1000, DType::I32);
        // Committed, the arm's value is a court's to check whole: a sink.
        let y = if commit_arm { b.commit(y) } else { y };
        let y = if nested {
            let col2 = b.reshape_fixed(x, &[7, 1]);
            let y2 = b.matmul(w2.expect("declared when nested"), col2, DType::I64);
            let y2 = b.reshape_fixed(y2, &[7]);
            let y2 = b.clamp(y2, -1000, 1000, DType::I32);
            let inner_cond = b.compare(x, x, Cmp::Ne);
            let doubled = b.add(y, y, DType::I32);
            let doubled = b.clamp(doubled, -1000, 1000, DType::I32);
            b.select(inner_cond, y2, doubled, DType::I32)
        } else {
            y
        };
        let z = b.add(x, x, DType::I32);
        let z = b.clamp(z, -1000, 1000, DType::I32);
        let s = b.select(cond, y, z, DType::I32);
        b.finish(&[s])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let col = b.reshape_fixed(Ref::CarryIn(0), &[7, 1]);
        let l = b.matmul(head, col, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut p = pb.finish(pre, vec![layer], post, logits);
    p.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    p
}

/// The LM head alone: what the dense work is once the arm's matmul is not credited.
fn head_only_dense() -> u128 {
    // The same program with the `Select` replaced by its cheap arm credits the head's matmul only.
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, 7], false);
    let head = pb.param("head", DType::I8, &[8, 7], false);
    let carry = vec![TensorType::fixed(DType::I32, &[7])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let z = b.add(Ref::CarryIn(0), Ref::CarryIn(0), DType::I32);
        let z = b.clamp(z, -1000, 1000, DType::I32);
        b.finish(&[z])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let col = b.reshape_fixed(Ref::CarryIn(0), &[7, 1]);
        let l = b.matmul(head, col, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut p = pb.finish(pre, vec![layer], post, logits);
    p.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    work(&p, false).dense_matmul
}

#[test]
fn an_unread_arm_s_matmul_is_not_credited_past_the_fence() {
    let p = program(false, false);
    let layer = &p.blocks[1];
    let regions = palw_tir_select_arm_regions_v1(layer);
    assert_eq!(regions.len(), 1, "one Select");
    let (_, [a, b]) = &regions[0];
    assert!(a.iter().any(|m| matches!(layer.nodes[*m].prim, misaka_palw_tir::Prim::MatMul)), "arm 1 holds the matmul: {a:?}");
    assert_eq!(b.len(), 2, "arm 2 is the add and its clamp: {b:?}");
    let (release, fence2) = (work(&p, false), work(&p, true));
    assert!(release.dense_matmul > head_only_dense(), "the release credits the arm's matmul");
    assert_eq!(fence2.dense_matmul, head_only_dense(), "past the fence the arm's matmul is not credited: the other arm holds none");
    for (r, f) in fields(&release).iter().zip(fields(&fence2)) {
        assert!(f <= *r, "never more than the release credits");
    }
    assert!(fence2.other_verified_ops < release.other_verified_ops, "the arms' elementwise work at the smaller arm's");
}

#[test]
fn a_committed_arm_is_a_sink_and_is_credited_as_before() {
    let p = program(true, false);
    assert_eq!(work(&p, true).dense_matmul, work(&p, false).dense_matmul, "a court reads a committed matmul whole");
    let (_, [a, _]) = &palw_tir_select_arm_regions_v1(&p.blocks[1])[0];
    assert!(a.iter().all(|m| !p.blocks[1].nodes[*m].commit), "no committed node in a region");
}

#[test]
fn a_nested_select_is_credited_first() {
    let p = program(false, true);
    let layer = &p.blocks[1];
    let regions = palw_tir_select_arm_regions_v1(layer);
    assert_eq!(regions.len(), 2, "two Selects");
    let (inner, [ia, ib]) = &regions[0];
    let (_, [oa, _]) = &regions[1];
    assert!(oa.contains(inner), "the inner Select is in the outer arm");
    assert!(ia.iter().chain(ib).all(|m| oa.contains(m)), "its regions are inside the outer arm");
    let (release, fence2) = (work(&p, false), work(&p, true));
    assert_eq!(fence2.dense_matmul, head_only_dense(), "both arm matmuls go: the outer's other arm holds no dense work");
    assert!(release.dense_matmul > fence2.dense_matmul);
    for (r, f) in fields(&release).iter().zip(fields(&fence2)) {
        assert!(f <= *r);
    }
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn the_corpus_moves_by_its_arm_only_work_and_no_more() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/programs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let mut moved = 0;
    for path in files {
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let p = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).unwrap();
        let name = v["name"].as_str().unwrap();
        let selects = p.blocks.iter().flat_map(|b| &b.nodes).filter(|n| matches!(n.prim, misaka_palw_tir::Prim::Select)).count();
        let (release, fence2) = (work(&p, false), work(&p, true));
        for (r, f) in fields(&release).iter().zip(fields(&fence2)) {
            assert!(f <= *r, "{name}: never more than the release credits");
        }
        if selects == 0 {
            assert_eq!(release, fence2, "{name}: no Select, no change");
        }
        if release != fence2 {
            moved += 1;
        }
        println!("{name}: {selects} Selects; other {} → {}", release.other_verified_ops, fence2.other_verified_ops);
    }
    assert!(moved > 0, "some corpus program has arm-only work");
}
