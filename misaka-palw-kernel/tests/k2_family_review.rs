//! **Family review (G14 task 8):** for every constraint family a shipped descriptor declares — is there a checker, a localizer, an
//! exact public court, public material, a DA/default path, a finite bound and a negative test? The reference fixtures are the
//! evidence: a lie at EVERY node of the reference classes (every primitive they contain) is localized to exactly that node and
//! convicted by the public court holding only the commitments. All 25 TIR v1 primitives are exercised; had any not been, the list
//! `MISSING_PRIM_TAGS` would name it as a gap.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{Claim, bump};
use misaka_palw_kernel::descriptor::{k2_tir_v1_descriptor, k2_tir_v2_descriptor, k2_tir_v3_descriptor};
use misaka_palw_kernel::family::{ConstraintFamilyV1, family_of_prim};
use misaka_palw_kernel::gate::{ProsecutionPolicyV1, public_prosecution_complete_v1};
use misaka_palw_kernel::public::ProfileMaterialV1;
use misaka_palw_kernel::verify::{ClaimVerdictV1, ScopeV1, TraceMaterialV1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN, Ref};
use misaka_palw_tir::types::Dim;
use misaka_palw_tir::{DType, MapParams, Rounding, Tensor, TensorType};
use misaka_palw_tir_sketch::fixture::{TirSketchFixtureV1, wide_v1, wide128_v1};

/// **The class that exercises what the other reference fixtures do not**: `Broadcast`, `Iota`, `Log2Floor` (through the L2 norm),
/// `IntLn` (through softplus) and `StateWrite` (a token shift). Weights are drawn by a fixed xorshift, nothing here is a model.
fn exotic_v1() -> TirSketchFixtureV1 {
    let (v, d) = (32u32, 4u32);
    let mut pb = ProgramBuilder::new(v, HISTORY_BOUND_V1_SMALL);
    let tok = pb.param("tok_embd", DType::I8, &[v, d], false);
    let lm = pb.param("output.w", DType::I8, &[v, d], false);
    let prev = pb.fixed_state("prev", DType::I16, &[d], -32767, 32767, true);
    let carry = vec![TensorType::fixed(DType::I32, &[d])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.cast(row, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = Ref::CarryIn(0);
        // IntExp + IntLn: softplus of a small Q24 argument.
        let xq = b.clamp(x, -(1 << 20), 1 << 20, DType::I32);
        let sp = b.softplus_q36(xq);
        let sp = b.shr(sp, 12, Rounding::HalfAwayFromZero, DType::I64);
        let sp = b.clamp(sp, 0, 32767, DType::I16);
        // Log2Floor + IntRsqrt: the L2 norm of the row.
        let x16 = b.clamp(x, -32767, 32767, DType::I16);
        let n = b.l2_norm_q15(x16);
        // StateWrite: the previous position's row, this one stored.
        let before = b.token_shift(prev, n);
        // Iota + Broadcast.
        let j = b.iota(DType::I32, &[Dim::Fixed(d)], 0, 0, 1);
        let five = b.c(DType::I32, 5);
        let bias = b.broadcast(five, &[Dim::Fixed(d)]);
        let off = b.add(j, bias, DType::I32);
        let t = b.add(before, n, DType::I32);
        let t = b.add(t, sp, DType::I32);
        let t = b.add(t, off, DType::I32);
        let out = b.clamp(t, -(1 << 20), 1 << 20, DType::I32);
        b.finish(&[out])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let h = b.clamp(Ref::CarryIn(0), -32767, 32767, DType::I16);
        let hc = b.reshape_fixed(h, &[d, 1]);
        let l = b.matmul(lm, hc, DType::I32);
        let l = b.reshape_fixed(l, &[v]);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let program = pb.finish(pre, vec![layer], post, logits);
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    let mut draw = |n: usize| -> Vec<i128> {
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x % 255) as i128 - 127
            })
            .collect()
    };
    let mut tensors = std::collections::BTreeMap::new();
    for j in 0..2u16 {
        tensors.insert((j, None), Tensor::new(DType::I8, vec![v as usize, d as usize], draw((v * d) as usize)).unwrap());
    }
    TirSketchFixtureV1 { program, params: MapParams { tensors } }
}

/// Every node of every occurrence at `position`: lie, verify, require the fault at that node and a conviction by the court.
fn every_node_lies(c: &Claim, position: u32) -> BTreeMap<u8, usize> {
    let mut by_prim: BTreeMap<u8, usize> = BTreeMap::new();
    for (s, (b, _)) in c.program.occurrences().iter().enumerate() {
        for (n, node) in c.program.blocks[*b as usize].nodes.iter().enumerate() {
            let t = &c.trace.values[position as usize][s][n];
            if t.data.is_empty() {
                continue;
            }
            let mut lie = c.trace.clone();
            bump(&mut lie.values[position as usize][s][n], 0);
            let (v, court) =
                c.verify_scope(&lie, &TraceMaterialV1 { trace: &lie, params: &c.params }, &ScopeV1::Segments(vec![position / 2]));
            let ClaimVerdictV1::Fault(proof) = v else { panic!("a lie at ({position},{s},{n}) {:?} was not found: {v:?}", node.prim) };
            assert_eq!(
                (proof.position, proof.occurrence, proof.node),
                (position, s as u16, n as u16),
                "a lie at {:?} must localize to that node",
                node.prim
            );
            court(&proof).unwrap_or_else(|e| panic!("the public court dismissed a true lie at {:?}: {e:?}", node.prim));
            *by_prim.entry(node.prim.tag()).or_default() += 1;
        }
    }
    by_prim
}

#[test]
fn a_lie_at_every_node_of_the_reference_classes_is_localized_and_convicted() {
    let mut covered: BTreeMap<u8, usize> = BTreeMap::new();
    for (c, positions) in [
        (Claim::honest(), vec![1u32]),
        (Claim::of(wide128_v1(3), k2_tir_v2_descriptor()), vec![1]),
        (Claim::of(wide_v1(3), k2_tir_v2_descriptor()), vec![1]),
        (Claim::of(exotic_v1(), k2_tir_v1_descriptor()), vec![1, 2]),
    ] {
        for p in positions {
            for (tag, n) in every_node_lies(&c, p) {
                *covered.entry(tag).or_default() += n;
            }
        }
    }
    // Which TIR v1 primitives (tags 0..=24) have a negative test through a fixture, per family.
    let all: Vec<(u8, ConstraintFamilyV1)> = prim_samples().into_iter().map(|p| (p.tag(), family_of_prim(&p))).collect();
    let missing: Vec<u8> = all.iter().map(|(t, _)| *t).filter(|t| !covered.contains_key(t)).collect();
    // Per family, every primitive of it has a negative test.
    for f in ConstraintFamilyV1::ALL.iter().filter(|f| **f != ConstraintFamilyV1::MediaPipeline) {
        assert!(
            all.iter().filter(|(_, g)| g == f).all(|(t, _)| covered.contains_key(t)),
            "{}: a primitive has no negative test through a reference fixture",
            f.name()
        );
    }
    assert_eq!(missing, MISSING_PRIM_TAGS, "the primitives no reference fixture exercises are exactly this list (a GAP, not a PASS)");
}

/// Primitive tags no reference fixture contains: no negative test exists for them through a fixture.
const MISSING_PRIM_TAGS: [u8; 0] = [];
// (All 25 TIR v1 primitives are exercised: `exotic_v1` adds Broadcast, Iota, Log2Floor, IntLn and StateWrite to what the dense/MoE
// and wide fixtures contain. The media-pipeline family's edge kinds are covered in `k2_pipeline`.)

fn prim_samples() -> Vec<misaka_palw_tir::Prim> {
    use misaka_palw_tir::{Cmp, Prim, Rounding};
    vec![
        Prim::Reshape,
        Prim::Transpose { perm: vec![1, 0] },
        Prim::Slice { axis: 0, start: 0 },
        Prim::Concat { axis: 0 },
        Prim::Broadcast,
        Prim::Iota { axis: 0, start: 0, step: 1 },
        Prim::Gather { axis: 0, batch_dims: 0 },
        Prim::Cast,
        Prim::Add,
        Prim::Sub,
        Prim::Mul,
        Prim::MatMul,
        Prim::ReduceSum { axis: 0 },
        Prim::ReduceMax { axis: 0 },
        Prim::Div { rule: Rounding::Floor },
        Prim::Clamp { lo: 0, hi: 1 },
        Prim::Log2Floor,
        Prim::IntExp,
        Prim::IntRsqrt,
        Prim::IntLn,
        Prim::Compare { cmp: Cmp::Eq },
        Prim::Select,
        Prim::TopK { axis: 0, k: 1 },
        Prim::StateWrite { state: 0 },
        Prim::HistAppend { state: 0 },
    ]
}

#[test]
fn every_declared_family_has_a_checker_a_public_court_and_a_finite_bound() {
    let policy = ProsecutionPolicyV1 {
        court_deadline_daa: 20,
        max_sessions_per_claim: 1 << 10,
        max_public_bytes: 1 << 50,
        max_verifier_ram: 1 << 50,
        max_retained_state: 1 << 40,
    };
    let declared: BTreeSet<ConstraintFamilyV1> = [k2_tir_v1_descriptor(), k2_tir_v2_descriptor(), k2_tir_v3_descriptor()]
        .iter()
        .flat_map(|d| d.families.iter().map(|f| f.family))
        .collect();
    assert_eq!(declared.len(), ConstraintFamilyV1::ALL.len(), "every family is declared by some shipped descriptor");
    // The gate derives, for a plan over the reference class, a finite bound for every relation: no relation is unbounded or
    // private. (Pipelines' edges are bounded the same way in `k2_pipeline`.)
    let c = Claim::honest();
    let nodes: u64 = c.program.occurrences().iter().map(|(b, _)| c.program.blocks[*b as usize].nodes.len() as u64).sum();
    let bounds =
        public_prosecution_complete_v1(&c.descriptor, &c.plan, nodes, &ProfileMaterialV1::kernel_route(true), &policy).unwrap();
    assert!(bounds.max_opening_bytes > 0 && bounds.max_court_work > 0 && bounds.max_court_work <= c.descriptor.limits.max_court_work);
    let b = &c.plan.budgets;
    assert!(b.worst_court_bytes > 0 && b.worst_court_bytes <= c.descriptor.limits.max_court_bytes);
    for r in &c.plan.relations {
        let support = c.descriptor.support(r.family).expect("a plan relation's family is declared");
        assert_eq!((support.checker, support.court), (r.checker, r.court));
    }
    // Private material is never prosecutable: the gate refuses a profile whose weights/inputs/state are not public.
    let private = ProfileMaterialV1 { weights_public: false, ..ProfileMaterialV1::kernel_route(true) };
    assert!(public_prosecution_complete_v1(&c.descriptor, &c.plan, nodes, &private, &policy).is_err());
}
