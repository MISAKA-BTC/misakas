//! Synthetic full-graph resource/replay/court contract. No checkpoint, native
//! route, model-fidelity, network-load or ExecutionFinal claim.
use misaka_palw_kernel::{
    element::SegClaimContextV1,
    flat_court5::{FlatFaultV5, FlatHeaderV5, FlatOperandV5, FlatSegContextV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
    flat_court25::*,
    flat_detect5::*,
    flat_detect25::*,
    flat_replay6::FlatReplayLimitsV6,
    flat_replay25::*,
    flat_scope25::*,
    hash::Digest,
    job::DecodeRuleV1,
    merkle3::{LayoutV3, LeafOpeningV3, TreesV3},
    merkle5::{FlatLeafHashV5, FlatLeafValueV5, FlatTreeV5},
    seg::{NodeOpeningV1, SegmentedCommitmentsV1, prompt_root_of_ids_v1},
    trace::{ParamCommitmentsV1, TraceV1, trace_v1},
};
use misaka_palw_tir::{
    DType, Dim, MapParams, Ref, Tensor, TensorType, TirProgramV1, builder::ProgramBuilder, program::HISTORY_BOUND_V1_HELD,
};
use std::cell::Cell;

fn sparse(n: u32, layers: usize) -> TirProgramV1 {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let scores = b.param("independent.scores", DType::I32, &[n], false);
    let values = b.param("independent.values", DType::I32, &[n, 3], false);
    let state = b.fixed_state("accumulator", DType::I32, &[3], -100, 100, true);
    let mut pre = b.block("pre", vec![]);
    let x = pre.cast(Ref::Input(0), DType::I32);
    let x = pre.broadcast(x, &[Dim::Fixed(3)]);
    let pre = pre.finish(&[x]);
    let mut body = b.block("sparse-state", vec![TensorType::fixed(DType::I32, &[3])]);
    let ids = body.topk(scores, 0, 2);
    let selected = body.gather(values, ids, 0, 0);
    let sum = body.reduce_sum(selected, 0, DType::I64);
    let sum = body.reshape(sum, &[Dim::Fixed(3)]);
    let sum = body.add(sum, Ref::State(state), DType::I64);
    let sum = body.clamp(sum, -100, 100, DType::I32);
    let written = body.state_write(state, sum);
    let body = body.finish(&[written]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[3])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    b.finish(pre, vec![body; layers], post, 0)
}
fn sparse_params(n: u32) -> MapParams {
    let mut data = vec![0; n as usize];
    data[n as usize - 1] = 1;
    MapParams {
        tensors: [
            ((0, None), Tensor::new(DType::I32, vec![n as usize], data).unwrap()),
            ((1, None), Tensor::new(DType::I32, vec![n as usize, 3], (0..n as usize * 3).map(|i| (i % 3) as i128).collect()).unwrap()),
        ]
        .into(),
    }
}
#[allow(dead_code)]
fn history() -> TirProgramV1 {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let state = b.hist_state("history", DType::I16, &[2], 128, false);
    let mut pre = b.block("history-topk", vec![]);
    let x = pre.cast(Ref::Input(0), DType::I16);
    let row = pre.broadcast(x, &[Dim::Fixed(2)]);
    let h = pre.hist_append(state, row);
    let ids = pre.topk(h, 1, 1);
    let ids = pre.cast(ids, DType::I32);
    let sum = pre.reduce_sum(ids, 0, DType::I64);
    let sum = pre.reshape(sum, &[Dim::Fixed(1)]);
    let sum = pre.clamp(sum, -100, 100, DType::I32);
    let pre = pre.finish(&[sum]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[1])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    b.finish(pre, vec![], post, 0)
}
fn state_topk() -> (TirProgramV1, MapParams) {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let scores = b.param("independent.update", DType::I32, &[65], false);
    let state = b.fixed_state("scores", DType::I32, &[65], -10, 10, true);
    let mut pre = b.block("pre", vec![]);
    let x = pre.broadcast(Ref::Input(0), &[Dim::Fixed(2)]);
    let pre = pre.finish(&[x]);
    let mut body = b.block("state-topk", vec![TensorType::fixed(DType::Idx, &[2])]);
    let y = body.topk(Ref::State(state), 0, 2);
    body.state_write(state, scores);
    let body = body.finish(&[y]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::Idx, &[2])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![body; 3], post, 0);
    let mut scores = vec![0; 65];
    scores[64] = 1;
    (p, MapParams { tensors: [((0, None), Tensor::new(DType::I32, vec![65], scores).unwrap())].into() })
}
struct Trial {
    p: TirProgramV1,
    params: MapParams,
    pc: ParamCommitmentsV1,
    trace: TraceV1,
    prompt: Vec<u32>,
    generated: Vec<u32>,
    commitments: SegmentedCommitmentsV1,
    roots: Vec<Digest>,
    scope: FlatResourceScopeV25,
    envelope: FlatReplayEnvelopeV25,
}
impl Trial {
    fn new(p: TirProgramV1, params: MapParams, positions: usize) -> Self {
        TirProgramV1::decode_canonical(&p.encode()).unwrap();
        let prompt = vec![0; positions];
        let trace = trace_v1(&p, &params, &prompt).unwrap();
        let generated = vec![DecodeRuleV1::Greedy.select(&trace.values.last().unwrap().last().unwrap()[p.logits as usize]).unwrap()];
        let pc = ParamCommitmentsV1::of_v3(&params);
        let scope = flat_resource_scope_v25(&p, positions as u32, None).unwrap();
        let envelope = flat_replay_envelope_v25(&p, &scope, None).unwrap();
        let mut t = Self {
            p,
            params,
            pc,
            trace,
            prompt,
            generated,
            commitments: SegmentedCommitmentsV1::new(vec![]),
            roots: vec![],
            scope,
            envelope,
        };
        t.refresh();
        t
    }
    fn refresh(&mut self) {
        self.commitments = SegmentedCommitmentsV1::new(
            self.trace
                .values
                .iter()
                .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                .collect(),
        );
        self.roots = self.commitments.segment_roots();
    }
    fn context(&self) -> FlatContextV25<'_> {
        FlatContextV25 {
            scope: &self.scope,
            base: FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &self.p,
                    params: &self.pc,
                    segment_roots: &self.roots,
                    positions: self.prompt.len() as u32,
                    prompt_len: self.prompt.len() as u32,
                    prompt_root: prompt_root_of_ids_v1(&self.prompt),
                    inline_prompt: Some(&self.prompt),
                    generated: &self.generated,
                    decode: DecodeRuleV1::Greedy,
                    encoder: None,
                },
            },
        }
    }
    fn public(&self) -> Public<'_> {
        Public { t: self, mode: 0, calls: Cell::new(0), values: Cell::new(0) }
    }
    fn run(&self, public: &Public<'_>, session: &mut FlatReplaySessionV25) -> FlatReplayResultV25 {
        session.check(&self.context(), &self.envelope, public, &|i, l| self.params.tensors.get(&(i, l)).cloned(), &self.prompt)
    }
    fn fault(&self) -> FlatFaultV25 {
        let mut session = FlatReplaySessionV25::new(limits(&self.envelope, 1));
        let FlatReplayResultV25::Finding(FlatFindingV25::Fault(f)) = self.run(&self.public(), &mut session) else {
            panic!("no fault")
        };
        let bytes = f.to_bytes().unwrap();
        let f = FlatFaultV25::from_bytes(&bytes).unwrap();
        let bound = flat_fault_court_bounds_v25(&self.context(), &f).unwrap();
        assert!(bytes.len() as u64 <= bound.filing_bytes);
        assert!(bound.filing_bytes <= self.scope.worst_court_bytes);
        verify_flat_fault_v25(&self.context(), &f).unwrap();
        f
    }
}
fn limits(e: &FlatReplayEnvelopeV25, attempts: u32) -> FlatReplayLimitsV25 {
    let r = &e.reference;
    FlatReplayLimitsV25 {
        reference: FlatReplayLimitsV6 {
            position_tensor_bytes: r.position_tensor_bytes,
            operand_tensor_bytes: r.largest_operand_tensor_bytes,
            history_tensor_bytes: r.history_rows_tensor_bytes,
            parameter_hash_bytes: r.parameter_hash_bytes_per_attempt * attempts as u128,
            trace_hash_bytes: r.trace_hash_bytes_per_attempt * attempts as u128,
            node_reads: r.node_reads_per_attempt * attempts as u128,
            hash_reads: r.hash_reads_per_attempt * attempts as u128,
            reply_bytes: r.reply_bytes_per_attempt * attempts as u128,
            attempts,
        },
        topk_scan_work: e.topk_scan_work_per_attempt * attempts as u128,
        topk_auxiliary_bytes: e.topk_auxiliary_bytes,
        selected_value_reads: e.selected_value_reads_per_attempt * attempts as u128,
        header_reads: e.header_reads_per_attempt * attempts as u128,
        scope_checker_work: e.scope_checker_work_per_attempt * attempts as u128,
    }
}
struct Public<'a> {
    t: &'a Trial,
    mode: u8,
    calls: Cell<u32>,
    values: Cell<u32>,
}
impl FlatPublicReadsV5 for Public<'_> {
    fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
        self.calls.set(self.calls.get() + 1);
        if self.mode == 1 {
            return None;
        }
        let mut o = self.t.commitments.node_opening(u.position, u.occurrence, u.node)?;
        if self.mode == 2 {
            o.position += 1;
        }
        borsh::from_slice(&borsh::to_vec(&o).unwrap()).ok()
    }
    fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
        self.calls.set(self.calls.get() + 1);
        let t = &self.t.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
        let h = FlatTreeV5::of(t).ok()?.opening(u.leaf?)?;
        misaka_palw_kernel::merkle5::decode_flat_hash_v5(&borsh::to_vec(&h).unwrap()).ok()
    }
}
impl FlatPublicReadsV25 for Public<'_> {
    fn header(&self, r: FlatHeaderReadV25) -> Option<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        if self.mode == 7 {
            return None;
        }
        let u = r.node;
        let t = &self.t.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
        let shape = t.shape.iter().map(|d| *d as u64).collect();
        let header = if t.data.len() >= 64 {
            misaka_palw_kernel::flat_court5::FlatHeaderV5::Flat { dtype: t.dtype.tag(), shape, root: FlatTreeV5::of(t).ok()?.root() }
        } else {
            let tree = TreesV3::of(t);
            misaka_palw_kernel::flat_court5::FlatHeaderV5::Legacy {
                dtype: t.dtype.tag(),
                shape,
                row_root: tree.row_root(),
                col_root: tree.col_root(),
            }
        };
        let mut value = FlatHeaderValueV25 { version: 25, scope: r.scope, header };
        if self.mode == 8 {
            value.scope[0] ^= 1;
        }
        value.to_bytes().ok()
    }

    fn value(&self, r: FlatValueReadV25) -> Option<Vec<u8>> {
        self.values.set(self.values.get() + 1);
        if self.mode == 3 {
            return None;
        }
        if self.mode == 6 {
            return Some(vec![0; TYPED_VALUE_MAX_BYTES_V25 as usize + 1]);
        }
        let u = r.node;
        let t = &self.t.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
        let o = self.t.commitments.node_opening(u.position, u.occurrence, u.node)?;
        let mut value = FlatOperandV5 { node: Some(o), ..Default::default() };
        if t.data.len() >= 64 {
            let tree = FlatTreeV5::of(t).ok()?;
            value.flat.push(FlatLeafValueV5::of(t, &tree, tree.layout().leaf_of(r.element)?)?);
        } else {
            let (row, tile, _) = LayoutV3::of(&t.shape).row_leaf_of(r.element)?;
            value.legacy.push(LeafOpeningV3::of_trees(t, &TreesV3::of(t), 0, row, tile)?);
        }
        if self.mode == 4 {
            value.node.as_mut()?.commitment[0] ^= 1;
        }
        let mut bytes = FlatValueV25 { version: 25, scope: r.scope, value }.to_bytes().ok()?;
        if self.mode == 5 {
            bytes[0] = 6;
        }
        Some(bytes)
    }
}

use misaka_palw_tir::{
    Prim, Rounding,
    program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN},
};
use misaka_palw_tir_sketch::fixture::{TirSketchFixtureV1, dense_moe_v1, wide_v1, wide128_v1};
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
fn independent_mixed_replay_finds_all25_first_faults_and_all_honest_nodes() {
    let mut covered = std::collections::BTreeSet::new();
    for fx in [dense_moe_v1(7), wide_v1(3), wide128_v1(3), exotic_v1()] {
        let good = Trial::new(fx.program.clone(), fx.params.clone(), 2);
        let mut session = FlatReplaySessionV25::new(limits(&good.envelope, 1));
        assert_eq!(good.run(&good.public(), &mut session), FlatReplayResultV25::Finding(FlatFindingV25::Clean));
        for (s, (b, _)) in fx.program.occurrences().iter().enumerate() {
            for (n, node) in fx.program.blocks[*b as usize].nodes.iter().enumerate() {
                if covered.contains(&node.prim.tag()) {
                    continue;
                }
                let mut bad = Trial::new(fx.program.clone(), fx.params.clone(), 2);
                let t = &mut bad.trace.values[1][s][n];
                let e = t.data.len() - 1;
                t.data[e] = if t.dtype.contains(t.data[e] + 1) { t.data[e] + 1 } else { t.data[e] - 1 };
                bad.refresh();
                let f = bad.fault();
                if matches!(node.prim, Prim::TopK { .. }) {
                    assert!(matches!(f, FlatFaultV25::TopK(_)));
                } else {
                    let FlatFaultV25::Ordinary(f) = f else { panic!("ordinary tag {}", node.prim.tag()) };
                    assert_eq!(
                        (
                            f.output.node.as_ref().unwrap().position,
                            f.output.node.as_ref().unwrap().occurrence,
                            f.output.node.as_ref().unwrap().node
                        ),
                        (1, s as u16, n as u16)
                    );
                }
                covered.insert(node.prim.tag());
            }
        }
    }
    assert_eq!(covered, (0..25).collect());
}
#[test]
fn typed_reply_withholding_borrowing_codec_and_retry_limits_are_demands_not_convictions() {
    let mut t = Trial::new(sparse(129, 1), sparse_params(129), 2);
    // Ordinary Gather follows TopK; finder receives no corrupted coordinates.
    t.trace.values[1][1][1].data[0] += 1;
    t.refresh();
    for mode in [1, 2, 3, 4, 5, 6, 7, 8] {
        let mut public = t.public();
        public.mode = mode;
        let mut session = FlatReplaySessionV25::new(limits(&t.envelope, 1));
        let result = t.run(&public, &mut session);
        assert!(
            matches!(
                result,
                FlatReplayResultV25::Finding(
                    FlatFindingV25::Demand(_) | FlatFindingV25::ValueDemand(_) | FlatFindingV25::HeaderDemand(_)
                )
            ),
            "mode {mode}: {result:?}"
        );
        assert_eq!(session.attempts, 1);
        assert_eq!(t.run(&public, &mut session), FlatReplayResultV25::Limit("attempts"));
    }
    for (kind, lim) in (0..4).map(|k| {
        let mut l = limits(&t.envelope, 1);
        match k {
            0 => l.selected_value_reads = 0,
            1 => l.header_reads = 0,
            2 => l.reference.node_reads = 0,
            _ => l.scope_checker_work = 0,
        };
        (k, l)
    }) {
        let public = t.public();
        let mut session = FlatReplaySessionV25::new(lim);
        assert!(matches!(t.run(&public, &mut session), FlatReplayResultV25::Limit(_)), "kind {kind}");
        if kind >= 2 {
            assert_eq!(public.calls.get(), 0);
        }
    }
    let f = t.fault();
    let bytes = f.to_bytes().unwrap();
    assert_eq!(FlatFaultV25::from_bytes(&bytes).unwrap(), f);
    assert!(misaka_palw_kernel::flat_court7::FlatFaultV7::from_bytes(&bytes).is_err());
    for n in 0..bytes.len() {
        assert!(FlatFaultV25::from_bytes(&bytes[..n]).is_err());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(FlatFaultV25::from_bytes(&trailing).is_err());
    let mut scope = t.scope.clone();
    scope.metadata_derivation_work -= 1;
    let mut c = t.context();
    c.scope = &scope;
    assert!(verify_flat_fault_v25(&c, &f).is_err());
}
#[test]
fn mixed_decode_malformed_and_initial_public_topk_keep_distinct_legacy_courts() {
    let mut t = Trial::new(sparse(65, 1), sparse_params(65), 1);
    t.generated[0] = (t.generated[0] + 1) % 3;
    assert!(matches!(t.fault(), FlatFaultV25::Legacy(FlatFaultV5::Decode(_))));
    let mut t = Trial::new(sparse(65, 1), sparse_params(65), 1);
    t.trace.values[0][1][1].dtype = DType::I64;
    t.refresh();
    assert!(matches!(t.fault(), FlatFaultV25::Legacy(FlatFaultV5::Malformed(_))));
    let (p, m) = state_topk();
    let mut t = Trial::new(p, m, 1);
    t.trace.values[0][1][0].data[0] += 1;
    t.refresh();
    assert!(matches!(t.fault(), FlatFaultV25::Legacy(FlatFaultV5::Whole(_) | FlatFaultV5::Leaf(_))));
}
#[test]
fn public_value25_roundtrip_all_types_and_hostile_bounds_are_strict() {
    for dtype in DType::ALL {
        let mut t = Trial::new(sparse(65, 1), sparse_params(65), 1);
        let value = &mut t.trace.values[0][0][1];
        value.dtype = dtype;
        t.refresh();
        let public = t.public();
        let r = FlatValueReadV25 {
            scope: t.scope.root(),
            node: FlatReadV5 { position: 0, occurrence: 0, node: 1, leaf: None },
            element: 0,
        };
        let bytes = public.value(r).unwrap();
        let reply = FlatValueV25::from_bytes(&bytes).unwrap();
        assert_eq!(authenticated_output_point_v25(&reply.value, dtype, &[3], 0), Some(0));
        for n in 0..bytes.len() {
            assert!(FlatValueV25::from_bytes(&bytes[..n]).is_err());
        }
        let mut b = bytes;
        b.push(0);
        assert!(FlatValueV25::from_bytes(&b).is_err());
    }
    assert!(FlatValueV25::from_bytes(&vec![0; TYPED_VALUE_MAX_BYTES_V25 as usize + 1]).is_err());
    assert!(FlatHeaderValueV25::from_bytes(&vec![0; TYPED_HEADER_MAX_BYTES_V25 as usize + 1]).is_err());
}

#[test]
fn ordinary_relation_helper_descends_nonfirst_leaf_and_validates_coordinate_before_callbacks() {
    let mut pb = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let mut pre = pb.block("unknown-element", vec![]);
    let scalar = pre.cast(Ref::Input(0), DType::I32);
    let row = pre.broadcast(scalar, &[Dim::Fixed(256)]);
    let pre = pre.finish(&[row]);
    let mut post = pb.block("post", vec![TensorType::fixed(DType::I32, &[256])]);
    let logits = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(logits);
    let post = post.finish(&[]);
    let p = pb.finish(pre, vec![], post, 0);
    let mut trial = Trial::new(p, MapParams::default(), 1);
    let expected = trial.trace.values[0][0][1].clone();
    let inputs = vec![trial.trace.values[0][0][0].clone()];
    trial.trace.values[0][0][1].data[255] += 1;
    trial.refresh();
    let unit = FlatReadV5 { position: 0, occurrence: 0, node: 1, leaf: None };
    let public = trial.public();
    let FlatFindingV25::Fault(f) =
        locate_ordinary_relation_v25(&trial.context(), &public, unit, &expected, &|| Ok(inputs.clone()), &|| None)
    else {
        panic!("no ordinary fault")
    };
    let FlatFaultV25::Ordinary(point) = f else { panic!("wrong namespace") };
    assert_eq!(point.element, 255);
    assert_eq!(point.output.flat[0].opening.index, 3);
    assert_eq!(public.values.get(), 1);
    assert!(public.calls.get() > 2, "unknown divergent leaf requires hash descent");
    for invalid in [
        FlatReadV5 { position: 1, ..unit },
        FlatReadV5 { occurrence: u16::MAX, ..unit },
        FlatReadV5 { node: u16::MAX, ..unit },
        FlatReadV5 { leaf: Some(0), ..unit },
    ] {
        let public = trial.public();
        assert!(matches!(
            locate_ordinary_relation_v25(
                &trial.context(),
                &public,
                invalid,
                &expected,
                &|| panic!("own callback before coordinate validation"),
                &|| panic!("token callback before coordinate validation")
            ),
            FlatFindingV25::Inconsistent(_)
        ));
        assert_eq!(public.calls.get(), 0);
        assert_eq!(public.values.get(), 0);
    }
    let mut foreign_scope = trial.scope.clone();
    foreign_scope.max_positions += 1;
    let mut c = trial.context();
    c.scope = &foreign_scope;
    let public = trial.public();
    assert!(matches!(
        locate_ordinary_relation_v25(&c, &public, unit, &expected, &|| panic!("own callback before scope validation"), &|| None),
        FlatFindingV25::Inconsistent(_)
    ));
    assert_eq!(public.calls.get(), 0);
    let mut missing = trial.public();
    missing.mode = 3;
    assert!(matches!(
        locate_ordinary_relation_v25(&trial.context(), &missing, unit, &expected, &|| Ok(inputs.clone()), &|| None),
        FlatFindingV25::ValueDemand(_)
    ));
    let mut borrowed = trial.public();
    borrowed.mode = 4;
    assert!(matches!(
        locate_ordinary_relation_v25(&trial.context(), &borrowed, unit, &expected, &|| Ok(inputs.clone()), &|| None),
        FlatFindingV25::ValueDemand(_)
    ));
}

#[test]
fn public_header_authentication_binds_all_dtypes_both_grammars_and_malformed_metadata() {
    for dtype in DType::ALL {
        for n in [1, 63, 64, 127, 129] {
            let t = Tensor::new(dtype, vec![n], vec![0; n]).unwrap();
            let legacy = TreesV3::of(&t);
            let header = FlatHeaderValueV25 {
                version: 25,
                scope: [19; 64],
                header: FlatHeaderV5::Legacy {
                    dtype: dtype.tag(),
                    shape: vec![n as u64],
                    row_root: legacy.row_root(),
                    col_root: legacy.col_root(),
                },
            };
            assert!(header.authenticates(&misaka_palw_kernel::merkle3::tensor_commitment_v3(&t)));
            let mut borrowed = header.clone();
            borrowed.version = 7;
            assert!(!borrowed.authenticates(&misaka_palw_kernel::merkle3::tensor_commitment_v3(&t)));
            if n >= 64 {
                let tree = FlatTreeV5::of(&t).unwrap();
                let root = tree.commitment();
                let flat = FlatHeaderValueV25 {
                    version: 25,
                    scope: [19; 64],
                    header: FlatHeaderV5::Flat { dtype: dtype.tag(), shape: vec![n as u64], root: tree.root() },
                };
                assert!(flat.authenticates(&root));
                assert!(!header.authenticates(&root));
                let mut wrong = flat.clone();
                if let FlatHeaderV5::Flat { root, .. } = &mut wrong.header {
                    root[0] ^= 1
                };
                assert!(!wrong.authenticates(&root));
                let mut wrong = flat.clone();
                if let FlatHeaderV5::Flat { shape, .. } = &mut wrong.header {
                    shape.push(1)
                };
                assert!(!wrong.authenticates(&root));
            }
        }
    }
    // The bounded hash grammar must authenticate wrong metadata so a Header DA
    // answer can be prosecuted by the existing malformed court. No allocation
    // proportional to the dimension values takes place.
    for shape in [vec![0], vec![u64::MAX, 2], vec![1, 1, 1, 1]] {
        let mut h = misaka_palw_kernel::hash::keyed(misaka_palw_kernel::merkle3::TENSOR_COMMITMENT_DOMAIN_V3);
        h.update(&[DType::I16.tag(), shape.len() as u8]);
        for dim in &shape {
            h.update(&u64::to_le_bytes(*dim));
        }
        h.update(&[7; 64]).update(&[8; 64]);
        let root = misaka_palw_kernel::hash::finish(h);
        let value = FlatHeaderValueV25 {
            version: 25,
            scope: [0; 64],
            header: FlatHeaderV5::Legacy { dtype: DType::I16.tag(), shape, row_root: [7; 64], col_root: [8; 64] },
        };
        assert!(value.authenticates(&root));
        assert!(FlatHeaderValueV25::from_bytes(&value.to_bytes().unwrap()).unwrap().authenticates(&root));
    }
    let rank5 = FlatHeaderValueV25 {
        version: 25,
        scope: [0; 64],
        header: FlatHeaderV5::Legacy { dtype: DType::I16.tag(), shape: vec![1; 5], row_root: [7; 64], col_root: [8; 64] },
    };
    assert!(!rank5.authenticates(&[0; 64]));
    let invalid_dtype =
        FlatHeaderValueV25 { version: 25, scope: [0; 64], header: FlatHeaderV5::Flat { dtype: 255, shape: vec![64], root: [7; 64] } };
    assert!(!invalid_dtype.authenticates(&[0; 64]));
}

#[test]
fn new_typed_reply_carries_one_original_full_width_legacy_leaf_without_subtiling() {
    let tensor = Tensor::new(DType::I128, vec![4096], vec![1; 4096]).unwrap();
    let opening = LeafOpeningV3::of(&tensor, 0, 0, 0).unwrap();
    let commitment = misaka_palw_kernel::merkle3::tensor_commitment_v3(&tensor);
    assert!(opening.authenticates(&commitment));
    let value = FlatValueV25 {
        version: 25,
        scope: [9; 64],
        value: FlatOperandV5 {
            node: Some(NodeOpeningV1 {
                position: 0,
                occurrence: 0,
                node: 0,
                commitment,
                node_siblings: vec![],
                position_root: [3; 64],
                position_siblings: vec![],
            }),
            flat: vec![],
            legacy: vec![opening],
        },
    };
    let bytes = value.to_bytes().unwrap();
    assert!(bytes.len() > 64 << 10);
    assert!(bytes.len() <= TYPED_VALUE_MAX_BYTES_V25 as usize);
    assert_eq!(FlatValueV25::from_bytes(&bytes).unwrap(), value);
    assert_eq!(value.value.legacy[0].values.len(), 4096, "original source leaf cannot become a 64-element subtotal");
    assert!(FlatValueV25::from_bytes(&vec![0; (TYPED_VALUE_MAX_BYTES_V25 + 1) as usize]).is_err());
}
