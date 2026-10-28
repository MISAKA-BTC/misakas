//! Isolated opt-in courts; synthetic full semantic/authentication tests plus real program metadata.
//! No tests grant native execution or Final eligibility.
use misaka_palw_kernel::{
    element::SegClaimContextV1,
    flat_court5::{FlatOperandV5, FlatSegContextV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
    flat_point25::*,
    flat_scope25::*,
    hash::Digest,
    job::DecodeRuleV1,
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::{LayoutV3, LeafOpeningV3, TreesV3},
    merkle5::{FlatLeafValueV5, FlatTreeV5},
    seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1},
    trace::{ParamCommitmentsV1, SourceV1, TraceV1, WiringV1, trace_v1},
    verify::DismissalV1,
};
use misaka_palw_tir::{
    DType, MapParams, Prim, Ref, Rounding, Tensor, TensorType, TirProgramV1,
    builder::ProgramBuilder,
    program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN},
    types::Dim,
};
use misaka_palw_tir_sketch::fixture::{TirSketchFixtureV1, dense_moe_v1, wide_v1, wide128_v1};
use std::collections::BTreeSet;
struct Fixture {
    p: TirProgramV1,
    params: MapParams,
    pc: ParamCommitmentsV1,
    honest: TraceV1,
    trace: TraceV1,
    commit: SegmentedCommitmentsV1,
    roots: Vec<Digest>,
    tokens: Vec<u32>,
}
impl Fixture {
    fn new(fx: TirSketchFixtureV1, tokens: Vec<u32>) -> Self {
        TirProgramV1::decode_canonical(&fx.program.encode()).unwrap();
        let honest = trace_v1(&fx.program, &fx.params, &tokens).unwrap();
        let mut f = Self {
            p: fx.program,
            pc: ParamCommitmentsV1::of_v3(&fx.params),
            params: fx.params,
            trace: honest.clone(),
            honest,
            commit: SegmentedCommitmentsV1::new(vec![]),
            roots: vec![],
            tokens,
        };
        f.refresh();
        f
    }
    fn refresh(&mut self) {
        self.commit = SegmentedCommitmentsV1::new(
            self.trace
                .values
                .iter()
                .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                .collect(),
        );
        self.roots = self.commit.segment_roots();
    }
    fn context(&self) -> PointContextV25<'_> {
        PointContextV25 {
            court_id: ordinary_point_court_id_v25(),
            base: FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &self.p,
                    params: &self.pc,
                    segment_roots: &self.roots,
                    positions: self.tokens.len() as u32,
                    prompt_len: self.tokens.len() as u32,
                    prompt_root: prompt_root_of_ids_v1(&self.tokens),
                    inline_prompt: Some(&self.tokens),
                    generated: &[0],
                    decode: DecodeRuleV1::Greedy,
                    encoder: None,
                },
            },
        }
    }
    fn sources(&self, p: u32, s: u16, n: u16) -> Vec<SourceV1> {
        let w = WiringV1::new(&self.p).unwrap();
        let node = w.node(s, n);
        let mut sources = (0..node.inputs.len()).map(|i| w.input_source(&self.tokens, p, s, n, i).unwrap()).collect::<Vec<_>>();
        if matches!(node.prim, Prim::HistAppend { .. }) && p > 0 {
            sources.push(SourceV1::Node { position: p - 1, occurrence: s, node: n });
        }
        sources
    }
    fn own(&self, p: u32, s: u16, n: u16) -> Vec<Tensor> {
        self.sources(p, s, n)
            .iter()
            .map(|src| match src {
                SourceV1::Node { position, occurrence, node } => {
                    self.honest.values[*position as usize][*occurrence as usize][*node as usize].clone()
                }
                SourceV1::Param { index, layer } => self.params.tensors[&(*index, *layer)].clone(),
                SourceV1::Const(j) => {
                    let c = &self.p.consts[*j as usize];
                    Tensor::from_le_bytes(c.dtype, &c.shape.iter().map(|n| *n as usize).collect::<Vec<_>>(), &c.data).unwrap()
                }
                SourceV1::Public(v) => Tensor::scalar(DType::Idx, *v as i128).unwrap(),
                SourceV1::Zeros { dtype, shape } => Tensor::zeros(*dtype, shape),
                _ => panic!(),
            })
            .collect()
    }
    fn open(&self, t: &Tensor, node: Option<(u32, u16, u16)>, kind: PointOperandKindV25, leaves: &[(u64, u64)]) -> FlatOperandV5 {
        let opening = node.map(|(p, s, n)| self.commit.node_opening(p, s, n).unwrap());
        if kind == PointOperandKindV25::Public {
            return FlatOperandV5::default();
        }
        if kind == PointOperandKindV25::Flat {
            let tree = FlatTreeV5::of(t).unwrap();
            FlatOperandV5 {
                node: opening,
                flat: leaves.iter().map(|(i, _)| FlatLeafValueV5::of(t, &tree, *i).unwrap()).collect(),
                legacy: vec![],
            }
        } else {
            let axis = if kind == PointOperandKindV25::Column { AXIS_COL } else { AXIS_ROW };
            let trees = TreesV3::of(t);
            FlatOperandV5 {
                node: opening,
                flat: vec![],
                legacy: leaves.iter().map(|(a, b)| LeafOpeningV3::of_trees(t, &trees, axis, *a, *b).unwrap()).collect(),
            }
        }
    }
    fn proof(&self, p: u32, s: u16, n: u16, e: u64) -> FlatPointFaultV25 {
        let own = self.own(p, s, n);
        let deps = point_dependencies_v25(&self.context(), p, s, n, e, &own)
            .unwrap_or_else(|err| panic!("p{p} s{s} n{n} e{e} {:?}: {err:?}", WiringV1::new(&self.p).unwrap().node(s, n).prim));
        let sources = self.sources(p, s, n);
        let inputs = own
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let node = match sources[i] {
                    SourceV1::Node { position, occurrence, node } => Some((position, occurrence, node)),
                    _ => None,
                };
                self.open(t, node, deps.kinds[i], &deps.leaves[i])
            })
            .collect();
        let t = &self.trace.values[p as usize][s as usize][n as usize];
        let kind = if t.data.len() >= 64 { PointOperandKindV25::Flat } else { PointOperandKindV25::Row };
        let coordinate = if kind == PointOperandKindV25::Flat {
            (FlatTreeV5::of(t).unwrap().layout().leaf_of(e).unwrap(), 0)
        } else {
            let (a, b, _) = LayoutV3::of(&t.shape).row_leaf_of(e).unwrap();
            (a, b)
        };
        FlatPointFaultV25 { version: 25, element: e, output: self.open(t, Some((p, s, n)), kind, &[coordinate]), inputs, token: None }
    }
    fn bump(&mut self, p: u32, s: u16, n: u16, e: u64) {
        let t = &mut self.trace.values[p as usize][s as usize][n as usize];
        let old = t.data[e as usize];
        t.data[e as usize] = if t.dtype.contains(old + 1) { old + 1 } else { old - 1 };
        self.refresh();
    }
}

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

#[test]
fn all_24_ordinary_primitives_dismiss_honest_and_convict_exact_authenticated_wrong_points() {
    let mut covered = BTreeSet::new();
    let mut checked = 0;
    for fx in [dense_moe_v1(7), wide_v1(3), wide128_v1(3), exotic_v1()] {
        let mut f = Fixture::new(fx, vec![3, 17, 9]);
        for p in [0, 1, 2] {
            for (s, (block, _)) in f.p.occurrences().clone().iter().enumerate() {
                for n in 0..f.p.blocks[*block as usize].nodes.len() {
                    let prim = f.p.blocks[*block as usize].nodes[n].prim.clone();
                    if matches!(prim, Prim::TopK { .. }) {
                        continue;
                    }
                    let (s, n) = (s as u16, n as u16);
                    let len = f.honest.values[p as usize][s as usize][n as usize].data.len();
                    for e in [0, len / 2, len - 1].into_iter().collect::<BTreeSet<_>>() {
                        let honest = f.proof(p, s, n, e as u64);
                        let wire = honest.to_bytes().unwrap();
                        assert_eq!(FlatPointFaultV25::from_bytes(&wire).unwrap(), honest);
                        assert_eq!(
                            verify_flat_point_fault_v25(&f.context(), &honest),
                            Err(DismissalV1::NoFault),
                            "honest {prim:?} p{p} s{s} n{n}"
                        );
                        f.bump(p, s, n, e as u64);
                        let lie = f.proof(p, s, n, e as u64);
                        assert_eq!(
                            verify_flat_point_fault_v25(
                                &f.context(),
                                &FlatPointFaultV25::from_bytes(&lie.to_bytes().unwrap()).unwrap()
                            ),
                            Ok(()),
                            "false {prim:?} p{p} s{s} n{n}"
                        );
                        f.trace = f.honest.clone();
                        f.refresh();
                        covered.insert(prim.tag());
                        checked += 1;
                    }
                }
            }
        }
    }
    let all = (0..=24).filter(|n| *n != Prim::TopK { axis: 0, k: 1 }.tag()).collect::<BTreeSet<_>>();
    assert_eq!(covered, all);
    assert!(checked > 500);
}
fn cast_fixture(dtype: DType, len: u32) -> TirSketchFixtureV1 {
    let source = if dtype == DType::I128 { DType::I64 } else { dtype };
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let p = b.param("values", source, &[len], false);
    let (pre, out) = {
        let mut block = b.block("pre", vec![]);
        let x = block.cast(p, dtype);
        let y = block.cast(x, dtype);
        let carry = block.cast(y, DType::I32);
        let Ref::Node(out) = carry else { panic!() };
        (block.finish(&[carry]), out)
    };
    let carry = vec![b.blocks[pre as usize].nodes[out as usize].out.clone()];
    let (post, logits) = {
        let mut block = b.block("post", carry);
        let x = block.cast(Ref::CarryIn(0), DType::I32);
        block.commit(x);
        let Ref::Node(n) = x else { panic!() };
        (block.finish(&[]), n)
    };
    TirSketchFixtureV1 {
        program: b.finish(pre, vec![], post, logits),
        params: MapParams {
            tensors: [((0, None), Tensor::new(source, vec![len as usize], (0..len).map(|n| (n % 11) as i128).collect()).unwrap())]
                .into(),
        },
    }
}
#[test]
fn all_six_dtypes_and_flat_tail_thresholds_use_one_point_without_changing_commitments() {
    for dtype in DType::ALL {
        for len in [1, 63, 64, 127, 128, 129] {
            let mut f = Fixture::new(cast_fixture(dtype, len), vec![0]);
            let scope = flat_resource_scope_v25(&f.p, 1, None).unwrap();
            check_flat_resource_scope_v25(&f.p, 1, None, &scope).unwrap();
            for n in [0, 1] {
                for e in [0, len / 2, len - 1].into_iter().collect::<BTreeSet<_>>() {
                    let honest = f.proof(0, 0, n, e as u64);
                    assert_eq!(verify_flat_point_fault_v25(&f.context(), &honest), Err(DismissalV1::NoFault));
                    f.bump(0, 0, n, e as u64);
                    let lie = f.proof(0, 0, n, e as u64);
                    assert_eq!(
                        verify_flat_point_fault_v25(&f.context(), &FlatPointFaultV25::from_bytes(&lie.to_bytes().unwrap()).unwrap()),
                        Ok(())
                    );
                    f.trace = f.honest.clone();
                    f.refresh();
                }
            }
        }
    }
}
fn matrix_fixture() -> TirSketchFixtureV1 {
    let mut b = ProgramBuilder::new(128, HISTORY_BOUND_V1_SMALL);
    let x = b.param("x", DType::I16, &[2, 65], false);
    let w = b.param("w", DType::I8, &[65, 67], false);
    let (pre, out) = {
        let mut block = b.block("pre", vec![]);
        let x = block.cast(x, DType::I16);
        let y = block.matmul(x, w, DType::I32);
        let Ref::Node(n) = y else { panic!() };
        (block.finish(&[y]), n)
    };
    let carry = vec![b.blocks[pre as usize].nodes[out as usize].out.clone()];
    let (post, logits) = {
        let mut block = b.block("post", carry);
        let y = block.reshape_fixed(Ref::CarryIn(0), &[134]);
        let z = block.slice(y, 0, 0, 128);
        block.commit(z);
        let Ref::Node(n) = z else { panic!() };
        (block.finish(&[]), n)
    };
    TirSketchFixtureV1 {
        program: b.finish(pre, vec![], post, logits),
        params: MapParams {
            tensors: [
                ((0, None), Tensor::new(DType::I16, vec![2, 65], vec![2; 130]).unwrap()),
                ((1, None), Tensor::new(DType::I8, vec![65, 67], vec![3; 65 * 67]).unwrap()),
            ]
            .into(),
        },
    }
}
#[test]
fn column_parameter_proofs_are_explicitly_derived_and_borrowed_missing_or_wrong_axes_dismiss() {
    let mut f = Fixture::new(matrix_fixture(), vec![0]);
    let scope = point_relation_scope_v25(&f.p, 1, None, 0, 0, 1).unwrap();
    assert_eq!(scope.operand_kinds, [PointOperandKindV25::Flat, PointOperandKindV25::Column]);
    assert_eq!(scope.operand_leaves[1], 1);
    let honest = f.proof(0, 0, 1, 133);
    assert_eq!(verify_flat_point_fault_v25(&f.context(), &honest), Err(DismissalV1::NoFault));
    f.bump(0, 0, 1, 133);
    let lie = f.proof(0, 0, 1, 133);
    assert_eq!(verify_flat_point_fault_v25(&f.context(), &lie), Ok(()));
    let mut wrong = lie.clone();
    wrong.inputs[1].legacy[0].axis = AXIS_ROW;
    assert!(matches!(verify_flat_point_fault_v25(&f.context(), &wrong), Err(DismissalV1::NotAuthentic(_))));
    let mut wrong = lie.clone();
    wrong.inputs[1].legacy.clear();
    assert!(matches!(verify_flat_point_fault_v25(&f.context(), &wrong), Err(DismissalV1::NotAuthentic(_))));
    let mut wrong = lie.clone();
    wrong.inputs[0].node = wrong.output.node.clone();
    assert!(matches!(verify_flat_point_fault_v25(&f.context(), &wrong), Err(DismissalV1::NotAuthentic(_))));
    let mut wrong = lie.clone();
    wrong.output.node.as_mut().unwrap().position = 1;
    assert!(matches!(verify_flat_point_fault_v25(&f.context(), &wrong), Err(DismissalV1::NotAuthentic(_))));
    let mut wrong = lie.clone();
    wrong.output.flat[0].bytes[0] ^= 1;
    assert!(matches!(verify_flat_point_fault_v25(&f.context(), &wrong), Err(DismissalV1::NotAuthentic(_))));
    let mut wrong = lie.clone();
    wrong.inputs[1].legacy[0].shape[0] = 64;
    assert!(matches!(verify_flat_point_fault_v25(&f.context(), &wrong), Err(DismissalV1::NotAuthentic(_))));
    let mut wrong = lie.clone();
    let duplicate = wrong.inputs[1].legacy[0].clone();
    wrong.inputs[1].legacy.push(duplicate);
    assert!(matches!(verify_flat_point_fault_v25(&f.context(), &wrong), Err(DismissalV1::NotAuthentic(_))));
    let mut c = f.context();
    c.court_id = [0; 64];
    assert!(matches!(verify_flat_point_fault_v25(&c, &lie), Err(DismissalV1::NotAuthentic(_))));
    let mut pc = f.pc.clone();
    pc.by_instance.insert((1, None), [3; 64]);
    let mut c = f.context();
    c.base.claim.params = &pc;
    assert!(matches!(verify_flat_point_fault_v25(&c, &lie), Err(DismissalV1::NotAuthentic(_))));
    let bytes = lie.to_bytes().unwrap();
    for end in 0..bytes.len() {
        assert!(FlatPointFaultV25::from_bytes(&bytes[..end]).is_err());
    }
    let mut wrong = bytes.clone();
    wrong.extend([0]);
    assert!(FlatPointFaultV25::from_bytes(&wrong).is_err());
    let mut wrong = bytes.clone();
    wrong[0] = 7;
    assert!(FlatPointFaultV25::from_bytes(&wrong).is_err());
    let mut wrong = vec![25, 0];
    wrong.extend(0u64.to_le_bytes());
    wrong.push(0);
    wrong.extend(u32::MAX.to_le_bytes());
    assert!(FlatPointFaultV25::from_bytes(&wrong).is_err());
    assert!(FlatPointFaultV25::from_bytes(&vec![0; 1 + POINT_FAULT_MAX_BYTES_V25 as usize]).is_err());
    let mut admitted = flat_resource_scope_v25(&f.p, 1, None).unwrap();
    admitted.relations[0].work -= 1;
    assert!(check_flat_resource_scope_v25(&f.p, 1, None, &admitted).is_err());
}
fn history_fixture(row: u32) -> TirSketchFixtureV1 {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let values = b.param("row", DType::I16, &[row], false);
    let state = b.hist_state("history", DType::I16, &[row], 128, false);
    let (pre, out) = {
        let mut block = b.block("pre", vec![]);
        let x = block.cast(values, DType::I16);
        let y = block.hist_append(state, x);
        let y = block.transpose(y, &[1, 0]);
        let z = block.transpose(y, &[1, 0]);
        let reduced = block.reduce_sum(z, 0, DType::I32);
        let Ref::Node(n) = reduced else { panic!() };
        (block.finish(&[reduced]), n)
    };
    let carry = vec![b.blocks[pre as usize].nodes[out as usize].out.clone()];
    let (post, logits) = {
        let mut block = b.block("post", carry);
        let y = block.reshape_fixed(Ref::CarryIn(0), &[row]);
        block.commit(y);
        let Ref::Node(n) = y else { panic!() };
        (block.finish(&[]), n)
    };
    TirSketchFixtureV1 {
        program: b.finish(pre, vec![], post, logits),
        params: MapParams {
            tensors: [((0, None), Tensor::new(DType::I16, vec![row as usize], vec![2; row as usize]).unwrap())].into(),
        },
    }
}
#[test]
fn history_scope_covers_every_position_and_tail_not_only_the_maximum_shape() {
    for row in [1, 2, 3, 63, 64] {
        let mut f = Fixture::new(history_fixture(row), vec![0; 128]);
        let scope = flat_resource_scope_v25(&f.p, 128, None).unwrap();
        for p in 0..128 {
            for (s, (b, _)) in f.p.occurrences().iter().enumerate() {
                for n in 0..f.p.blocks[*b as usize].nodes.len() {
                    let bound = point_relation_scope_v25(&f.p, 128, None, p, s as u16, n as u16).unwrap();
                    let admitted = &scope.relations.iter().find(|r| r.occurrence == s as u16 && r.node == n as u16).unwrap();
                    assert!(bound.filing_bytes + 3 <= admitted.filing_bytes, "row{row} p{p} s{s} n{n} bytes");
                    assert!(bound.work + 384 <= admitted.work, "row{row} p{p} s{s} n{n} work");
                    assert!(bound.scratch_bytes + 48 <= admitted.scratch_bytes, "row{row} p{p} s{s} n{n} scratch");
                    for (a, b) in bound.operand_leaves.iter().zip(&admitted.operand_leaves) {
                        assert!(a <= b);
                    }
                }
            }
        }
        for p in [0, 1, 62, 63, 64, 126, 127] {
            for n in [1, 2, 3] {
                let len = f.honest.values[p as usize][0][n as usize].data.len();
                for e in [0, len - 1] {
                    assert_eq!(verify_flat_point_fault_v25(&f.context(), &f.proof(p, 0, n, e as u64)), Err(DismissalV1::NoFault));
                    f.bump(p, 0, n, e as u64);
                    assert_eq!(verify_flat_point_fault_v25(&f.context(), &f.proof(p, 0, n, e as u64)), Ok(()));
                    f.trace = f.honest.clone();
                    f.refresh();
                }
            }
        }
    }
}
#[test]
fn semantic_refusal_has_no_honest_value_and_missing_material_is_not_conviction() {
    let mut b = ProgramBuilder::new(128, HISTORY_BOUND_V1_SMALL);
    let x = b.param("values", DType::I16, &[1], false);
    let (pre, out) = {
        let mut block = b.block("pre", vec![]);
        let y = block.cast(x, DType::I8);
        let Ref::Node(n) = y else { panic!() };
        (block.finish(&[y]), n)
    };
    let carry = vec![b.blocks[pre as usize].nodes[out as usize].out.clone()];
    let (post, logits) = {
        let mut block = b.block("post", carry);
        let x = block.cast(Ref::CarryIn(0), DType::I32);
        block.commit(x);
        let Ref::Node(n) = x else { panic!() };
        (block.finish(&[]), n)
    };
    let mut f = Fixture::new(
        TirSketchFixtureV1 {
            program: b.finish(pre, vec![], post, logits),
            params: MapParams { tensors: [((0, None), Tensor::new(DType::I16, vec![1], vec![7]).unwrap())].into() },
        },
        vec![0],
    );
    f.params.tensors.get_mut(&(0, None)).unwrap().data[0] = 128;
    f.pc = ParamCommitmentsV1::of_v3(&f.params);
    let deps = point_dependencies_v25(&f.context(), 0, 0, 0, 0, &f.own(0, 0, 0)).unwrap();
    assert_eq!(deps.expected, None);
    let proof = f.proof(0, 0, 0, 0);
    assert_eq!(verify_flat_point_fault_v25(&f.context(), &proof), Ok(()));
    let mut missing = proof;
    missing.inputs[0].legacy.clear();
    assert!(matches!(verify_flat_point_fault_v25(&f.context(), &missing), Err(DismissalV1::NotAuthentic(_))));
}
#[test]
fn actual_pinned_qwen_complete_metadata_scope_has_a_distinct_scalar_court_set() {
    let bytes = include_bytes!("../../../../docs/design/palw/tir/evidence/qwen25-real-pilot-program.tir");
    let p = TirProgramV1::decode_canonical(bytes).unwrap();
    let refusal = misaka_palw_kernel::flat_scope9::first_legacy_ordinary_refusal_v9(&p, 32, None).unwrap().unwrap();
    assert_eq!((refusal.block, refusal.occurrence, refusal.node), (1, 1, 27));
    assert_eq!(refusal.primitive, "MatMul");
    assert!(refusal.point_replacement.is_ok());
    let scope = flat_resource_scope_v25(&p, 32, None).unwrap();
    assert_eq!(scope.relations.len(), 7658);
    assert_eq!(scope.max_positions, 32);
    assert_eq!(scope.profile, flat_trace_profile_id_v5());
    assert_ne!(scope.court_set, misaka_palw_kernel::flat_scope7::flat_court_set_id_v7());
    assert!(scope.worst_court_bytes <= POINT_FAULT_MAX_BYTES_V25);
    assert!(scope.worst_court_work <= POINT_FAULT_MAX_WORK_V25);
    assert!(scope.worst_court_scratch_bytes <= POINT_FAULT_MAX_SCRATCH_V25);
    assert!(misaka_palw_kernel::flat_scope7::flat_resource_scope_v7(&p, 32, None).is_err());
}

#[test]
fn encoder_maximum_axis_noninline_large_index_padding_and_count_remain_within_point_scope() {
    use misaka_palw_kernel::{seg::PromptTileOpeningV1, seg_encoder::*};
    assert_eq!(misaka_palw_kernel::seg::PROMPT_TILE_IDS_V1, 4096);
    let mut pb = ProgramBuilder::new(512, HISTORY_BOUND_V1_SMALL);
    let ids = pb.param(ENCODER_IDS_PARAM_V1, DType::Idx, &[4096], false);
    let count = pb.param(ENCODER_COUNT_PARAM_V1, DType::Idx, &[], false);
    let mut pre = pb.block("job-inputs", vec![]);
    let x = pre.cast(ids, DType::I32);
    let c = pre.cast(count, DType::I32);
    let c = pre.broadcast(c, &[Dim::Fixed(4096)]);
    let sum = pre.add(x, c, DType::I64);
    let sum = pre.clamp(sum, 0, 1024, DType::I32);
    let ty = pre.ty(sum);
    let pre = pre.finish(&[sum]);
    let mut post = pb.block("post", vec![ty]);
    let out = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(out);
    let post = post.finish(&[]);
    let program = pb.finish(pre, vec![], post, 0);
    TirProgramV1::decode_canonical(&program.encode()).unwrap();
    let binding = encoder_binding_v1(&program).unwrap();
    let tokens = (0..257).map(|i| (i % 512) as u32).collect::<Vec<_>>();
    let params = MapParams::default();
    let pc = ParamCommitmentsV1::of_v3(&params);
    let honest = trace_encoder_v1(&program, &params, &binding, &tokens).unwrap();
    let input_ids = binding.ids(&tokens).unwrap();
    let input_count = binding.count(tokens.len() as u32).unwrap();
    for (node, element, own) in [
        (0, 0, vec![input_ids.clone()]),
        (0, 64, vec![input_ids.clone()]),
        (0, 256, vec![input_ids.clone()]),
        (0, 4095, vec![input_ids.clone()]),
        (1, 0, vec![input_count.clone()]),
    ] {
        let scope = point_relation_scope_v25(&program, 1, Some(&binding), 0, 0, node).unwrap();
        // Full encoder input is bounded by the same 4096-id prompt tile; no
        // 64-value internal-flat assumption may reduce its public-input charge.
        if node == 0 {
            assert!(scope.scratch_bytes >= (8 << 20) + 4096 * 16);
        }
        for wrong in [false, true] {
            let mut trace = honest.clone();
            if wrong {
                trace.values[0][0][node as usize].data[element as usize] += 1;
            }
            let commitments = SegmentedCommitmentsV1::new(
                trace
                    .values
                    .iter()
                    .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                    .collect(),
            );
            let roots = commitments.segment_roots();
            let context = PointContextV25 {
                court_id: ordinary_point_court_id_v25(),
                base: FlatSegContextV5 {
                    profile: flat_trace_profile_id_v5(),
                    claim: SegClaimContextV1 {
                        program: &program,
                        params: &pc,
                        segment_roots: &roots,
                        positions: 1,
                        prompt_len: tokens.len() as u32,
                        prompt_root: prompt_root_of_ids_v1(&tokens),
                        inline_prompt: None,
                        generated: &[0],
                        decode: DecodeRuleV1::Greedy,
                        encoder: Some(binding),
                    },
                },
            };
            let t = &trace.values[0][0][node as usize];
            let output_node = commitments.node_opening(0, 0, node).unwrap();
            let mut output = FlatOperandV5 { node: Some(output_node), ..Default::default() };
            if t.data.len() >= 64 {
                let tree = FlatTreeV5::of(t).unwrap();
                output.flat.push(FlatLeafValueV5::of(t, &tree, tree.layout().leaf_of(element).unwrap()).unwrap());
            } else {
                output.legacy.push(LeafOpeningV3::of_trees(t, &TreesV3::of(t), AXIS_ROW, 0, 0).unwrap());
            }
            let deps = point_dependencies_v25(&context, 0, 0, node, element, &own).unwrap();
            assert!(deps.leaves.iter().all(|v| v.is_empty()));
            let mut f = FlatPointFaultV25 {
                version: 25,
                element,
                output,
                inputs: vec![FlatOperandV5::default()],
                token: (node == 0).then(|| PromptTileOpeningV1::of(&tokens, 0).unwrap()),
            };
            let wire = f.to_bytes().unwrap();
            assert!(wire.len() as u64 <= scope.filing_bytes);
            let result = verify_flat_point_fault_v25(&context, &FlatPointFaultV25::from_bytes(&wire).unwrap());
            if wrong {
                assert!(result.is_ok(), "{result:?}");
            } else {
                assert_eq!(result, Err(DismissalV1::NoFault));
            }
            if node == 0 {
                f.token.as_mut().unwrap().ids[64] ^= 1;
                assert!(matches!(verify_flat_point_fault_v25(&context, &f), Err(DismissalV1::NotAuthentic(_))));
                f.token = None;
                assert!(verify_flat_point_fault_v25(&context, &f).is_err());
            }
            let mut bad = context;
            bad.base.claim.prompt_len = 4097;
            assert!(matches!(verify_flat_point_fault_v25(&bad, &f), Err(DismissalV1::NotAuthentic(_))));
        }
    }
}

#[test]
fn checked_scope_and_codec_are_distinct_from_old_admission() {
    let mut f = Fixture::new(cast_fixture(DType::I32, 128), vec![3]);
    f.bump(0, 0, 0, 7);
    let proof = f.proof(0, 0, 0, 7);
    let wire = proof.to_bytes().unwrap();
    assert!(misaka_palw_kernel::flat_point9::FlatPointFaultV9::from_bytes(&wire).is_err());
    let checked = derive_checked_flat_resource_scope_v25(&f.p, 1, None).unwrap();
    let result = check_flat_point_fault_v25(&f.context(), &checked, &wire).unwrap();
    assert_eq!(result.scope_root(), checked.root());
    let (_, _, _, parts) = checked.paid_court_vector();
    assert!(parts > 0 && parts <= POINT_FAULT_MAX_PARTS_V25);
    let mut changed = checked.scope().clone();
    changed.max_positions += 1;
    assert!(check_flat_resource_scope_v25(&f.p, 1, None, &changed).is_err());
}
#[test]
fn large_complete_contiguous_reduce_has_new_scope_without_old_cap_change() {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let x = b.param("row", DType::I16, &[131072], false);
    let (pre, n) = {
        let mut a = b.block("pre", vec![]);
        let x = a.cast(x, DType::I16);
        let y = a.reduce_max(x, 0);
        let Ref::Node(n) = y else { panic!() };
        (a.finish(&[y]), n)
    };
    let carry = vec![b.blocks[pre as usize].nodes[n as usize].out.clone()];
    let (post, logits) = {
        let mut a = b.block("post", carry);
        let x = a.cast(Ref::CarryIn(0), DType::I32);
        let x = a.reshape_fixed(x, &[1]);
        a.commit(x);
        let Ref::Node(n) = x else { panic!() };
        (a.finish(&[]), n)
    };
    let p = b.finish(pre, vec![], post, logits);
    let new = point_relation_scope_v25(&p, 1, None, 0, 0, n).unwrap();
    assert_eq!(new.operand_reads, [131072]);
    assert!(new.operand_leaves[0] > 1024);
    assert!(misaka_palw_kernel::flat_point9::point_relation_scope_v9(&p, 1, None, 0, 0, n).is_err());
    derive_checked_flat_resource_scope_v25(&p, 1, None).unwrap();
}
