//! Synthetic full-graph resource/replay/court contract. No checkpoint, native
//! route, model-fidelity, network-load or ExecutionFinal claim.
#![allow(warnings)]
use misaka_palw_kernel::{
    element::SegClaimContextV1,
    flat_court5::{FlatFaultV5, FlatOperandV5, FlatSegContextV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
    flat_court7::*,
    flat_detect5::*,
    flat_detect7::*,
    flat_replay6::FlatReplayLimitsV6,
    flat_replay7::*,
    flat_scope5::flat_resource_scope_v5,
    flat_scope7::*,
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
    scope: FlatResourceScopeV7,
    envelope: FlatReplayEnvelopeV7,
}
impl Trial {
    fn new(p: TirProgramV1, params: MapParams, positions: usize) -> Self {
        TirProgramV1::decode_canonical(&p.encode()).unwrap();
        let prompt = vec![0; positions];
        let trace = trace_v1(&p, &params, &prompt).unwrap();
        let generated = vec![DecodeRuleV1::Greedy.select(&trace.values.last().unwrap().last().unwrap()[p.logits as usize]).unwrap()];
        let pc = ParamCommitmentsV1::of_v3(&params);
        let scope = flat_resource_scope_v7(&p, positions as u32, None).unwrap();
        let envelope = flat_replay_envelope_v7(&p, &scope, None).unwrap();
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
    fn context(&self) -> FlatContextV7<'_> {
        FlatContextV7 {
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
    fn run(&self, public: &Public<'_>, session: &mut FlatReplaySessionV7) -> FlatReplayResultV7 {
        session.check(&self.context(), &self.envelope, public, &|i, l| self.params.tensors.get(&(i, l)).cloned(), &self.prompt)
    }
    fn fault(&self) -> FlatFaultV7 {
        let mut session = FlatReplaySessionV7::new(limits(&self.envelope, 1));
        let FlatReplayResultV7::Finding(FlatFindingV7::Fault(f)) = self.run(&self.public(), &mut session) else { panic!("no fault") };
        let bytes = f.to_bytes().unwrap();
        let f = FlatFaultV7::from_bytes(&bytes).unwrap();
        let bound = flat_fault_court_bounds_v7(&self.context(), &f).unwrap();
        assert!(bytes.len() as u64 <= bound.filing_bytes);
        assert!(bound.filing_bytes <= self.scope.worst_court_bytes);
        verify_flat_fault_v7(&self.context(), &f).unwrap();
        f
    }
}
fn limits(e: &FlatReplayEnvelopeV7, attempts: u32) -> FlatReplayLimitsV7 {
    let r = &e.reference;
    FlatReplayLimitsV7 {
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
impl FlatPublicReadsV7 for Public<'_> {
    fn value(&self, r: FlatValueReadV7) -> Option<Vec<u8>> {
        self.values.set(self.values.get() + 1);
        if self.mode == 3 {
            return None;
        }
        if self.mode == 6 {
            return Some(vec![0; TOPK_VALUE_MAX_BYTES_V7 as usize + 1]);
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
        let mut bytes = FlatValueV7 { version: 7, value }.to_bytes().ok()?;
        if self.mode == 5 {
            bytes[0] = 6;
        }
        Some(bytes)
    }
}

#[test]
fn complete_scope_prices_ten_million_axis_sparse_gather_and_1024_state_layers_without_images() {
    let n = 10_485_760;
    let p = sparse(n, 1024);
    assert!(flat_resource_scope_v5(&p, HISTORY_BOUND_V1_HELD, None).is_err());
    let a = flat_resource_scope_v7(&p, 1, None).unwrap();
    let b = flat_resource_scope_v7(&p, HISTORY_BOUND_V1_HELD, None).unwrap();
    check_flat_resource_scope_v7(&p, HISTORY_BOUND_V1_HELD, None, &b).unwrap();
    assert_eq!(a.topk_selection_work_per_claim * HISTORY_BOUND_V1_HELD as u128, b.topk_selection_work_per_claim);
    let top = b.relations.iter().find(|r| r.court == FlatRelationCourtV7::TopKPointV7).unwrap();
    assert_eq!(top.occurrences, 1024);
    assert_eq!(top.selection_aux_bytes, 128);
    assert_eq!(b.topk_selection_work_per_claim, top.selection_work * 1024 * HISTORY_BOUND_V1_HELD as u128);
    assert_eq!(b.relation_instances_per_position, 2 + 1024 * 7 + 1);
    assert_eq!(b.relations.len(), 10);
    assert!(b.worst_court_bytes <= 1 << 20);
    assert!(b.worst_court_work <= 1 << 30);
    assert!(b.worst_court_scratch_bytes <= 32 << 20);
    assert_ne!(a.root(), b.root());
    println!(
        "flat-v7 synthetic metadata-only N={n} K=2 layers=1024 max_positions={} relations={} instances={} court_bytes={} court_work={} court_scratch={} scan_work={} aux={}; no allocated model/job, native or Final acceptance",
        b.max_positions,
        b.relations.len(),
        b.relation_instances_per_claim,
        b.worst_court_bytes,
        b.worst_court_work,
        b.worst_court_scratch_bytes,
        b.topk_selection_work_per_claim,
        b.topk_selection_aux_bytes
    );
}
#[test]
fn independent_full_scope_refuses_every_underpriced_or_rebound_field_and_other_oversized_relations() {
    let p = sparse(4097, 3);
    let s = flat_resource_scope_v7(&p, 3, None).unwrap();
    let mut variants = vec![];
    macro_rules! bad {
        ($field:ident) => {{
            let mut x = s.clone();
            if x.$field == 0 {
                x.$field += 1
            } else {
                x.$field -= 1
            };
            variants.push(x);
        }};
    }
    bad!(version);
    bad!(max_positions);
    bad!(relation_instances_per_position);
    bad!(relation_instances_per_claim);
    bad!(worst_court_bytes);
    bad!(worst_court_work);
    bad!(worst_court_scratch_bytes);
    bad!(worst_operand_leaves);
    bad!(trace_value_bytes_per_position);
    bad!(trace_value_bytes_per_claim);
    bad!(reference_tensor_bytes_per_position);
    bad!(node_commitments_bytes_per_claim);
    bad!(flat_tree_bytes_per_position);
    bad!(flat_hash_input_bytes_per_claim);
    bad!(topk_selection_work_per_claim);
    bad!(topk_selection_aux_bytes);
    bad!(topk_selected_value_reads_per_claim);
    bad!(topk_selected_reply_bytes_per_claim);
    for field in 0..3 {
        let mut x = s.clone();
        match field {
            0 => x.profile[0] ^= 1,
            1 => x.court_set[0] ^= 1,
            _ => x.program[0] ^= 1,
        };
        variants.push(x);
    }
    let mut x = s.clone();
    x.encoder = Some((0, 1));
    variants.push(x);
    let mut x = s.clone();
    x.relations.remove(0);
    variants.push(x);
    for field in 0..11 {
        let mut x = s.clone();
        let r = x.relations.iter_mut().find(|r| r.court == FlatRelationCourtV7::TopKPointV7).unwrap();
        match field {
            0 => r.block += 1,
            1 => r.node += 1,
            2 => r.occurrences -= 1,
            3 => r.court = FlatRelationCourtV7::V5,
            4 => r.filing_bytes -= 1,
            5 => r.work -= 1,
            6 => r.scratch_bytes -= 1,
            7 => r.operand_leaves.clear(),
            8 => r.selection_work -= 1,
            9 => r.selection_aux_bytes -= 1,
            _ => r.selected_value_reads -= 1,
        };
        variants.push(x);
    }
    for x in variants {
        assert!(check_flat_resource_scope_v7(&p, 3, None, &x).is_err());
        assert_ne!(x.root(), s.root());
    }
    assert!(flat_resource_scope_v7(&p, 0, None).is_err());
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let scores = b.param("scores", DType::I32, &[10_485_760], false);
    let mut pre = b.block("pre", vec![]);
    let ids = pre.topk(scores, 0, 2);
    let sum = pre.reduce_sum(scores, 0, DType::I128);
    let sum = pre.clamp(sum, -10, 10, DType::I32);
    let sum = pre.broadcast(sum, &[Dim::Fixed(2)]);
    let pre = pre.finish(&[ids, sum]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::Idx, &[2]), TensorType::fixed(DType::I32, &[2])]);
    let y = post.cast(Ref::CarryIn(1), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    assert!(misaka_palw_tir::validate::validate(&p).is_ok());
    assert!(flat_resource_scope_v7(&p, 1, None).is_err(), "small TopK proof cannot omit the inadmissible reduction");
}
#[test]
fn honest_sparse_graph_replays_and_late_topk_or_state_faults_reach_their_own_courts() {
    let mut t = Trial::new(sparse(4097, 2), sparse_params(4097), 3);
    let public = t.public();
    let mut session = FlatReplaySessionV7::new(limits(&t.envelope, 1));
    assert_eq!(t.run(&public, &mut session), FlatReplayResultV7::Finding(FlatFindingV7::Clean));
    assert_eq!(public.values.get(), 0);
    t.trace.values[2][2][0].data = vec![0, 4095];
    t.refresh();
    let FlatFaultV7::TopK(f) = t.fault() else { panic!("wrong court") };
    assert_eq!(f.output.node.as_ref().unwrap().position, 2);
    assert_eq!(f.output.node.as_ref().unwrap().occurrence, 2);
    assert!(matches!(f.kind, misaka_palw_kernel::flat_topk7::TopKFaultKindV7::Dominance { .. }));
    let mut t = Trial::new(sparse(4097, 2), sparse_params(4097), 3);
    t.trace.values[2][2][6].data[0] += 1;
    t.refresh();
    assert!(matches!(t.fault(), FlatFaultV7::V5(FlatFaultV5::Whole(_))));
}
#[test]
fn range_duplicate_and_dominance_faults_use_bounded_selected_value_replies() {
    for ids in [vec![0, 3], vec![0, 0], vec![1, 2]] {
        let mut t = Trial::new(sparse(3, 1), sparse_params(3), 1);
        t.trace.values[0][1][0].data = ids;
        t.refresh();
        let f = t.fault();
        let bytes = f.to_bytes().unwrap();
        assert!(bytes.len() < 32 << 10);
        assert!(FlatFaultV5::from_bytes(&bytes).is_err());
        for cut in 0..bytes.len() {
            assert!(FlatFaultV7::from_bytes(&bytes[..cut]).is_err());
        }
        let mut suffix = bytes.clone();
        suffix.push(0);
        assert!(FlatFaultV7::from_bytes(&suffix).is_err());
        let mut wrong = bytes;
        wrong[2] = 2;
        assert!(FlatFaultV7::from_bytes(&wrong).is_err());
    }
}
#[test]
fn missing_borrowed_wrong_version_and_oversized_material_never_produces_a_conviction() {
    let mut t = Trial::new(sparse(3, 1), sparse_params(3), 1);
    t.trace.values[0][1][0].data = vec![1, 2];
    t.refresh();
    for mode in 1..=6 {
        let mut public = t.public();
        public.mode = mode;
        let mut session = FlatReplaySessionV7::new(limits(&t.envelope, 1));
        assert!(matches!(
            t.run(&public, &mut session),
            FlatReplayResultV7::Finding(FlatFindingV7::Demand(_) | FlatFindingV7::ValueDemand(_))
        ));
        assert_eq!(session.attempts, 1);
        assert_eq!(session.topk_scan_work, t.envelope.topk_scan_work_per_attempt);
    }
    let public = t.public();
    let read = FlatValueReadV7 { node: FlatReadV5 { position: 0, occurrence: 1, node: 0, leaf: None }, element: 0 };
    let bytes = public.value(read).unwrap();
    for cut in 0..bytes.len() {
        assert!(FlatValueV7::from_bytes(&bytes[..cut]).is_err());
    }
    let mut decoded = FlatValueV7::from_bytes(&bytes).unwrap();
    decoded.value.legacy.push(decoded.value.legacy[0].clone());
    assert!(FlatValueV7::from_bytes(&borsh::to_vec(&decoded).unwrap()).is_err());
    let mut bytes = bytes;
    bytes.push(0);
    assert!(FlatValueV7::from_bytes(&bytes).is_err());
}
#[test]
fn session_limits_precede_callbacks_and_missing_material_spends_the_retry_budget() {
    let mut t = Trial::new(sparse(129, 1), sparse_params(129), 1);
    t.trace.values[0][1][0].data = vec![0, 127];
    t.refresh();
    for dimension in 0..7 {
        let public = t.public();
        let mut cap = limits(&t.envelope, 1);
        match dimension {
            0 => cap.reference.position_tensor_bytes = 0,
            1 => cap.reference.operand_tensor_bytes = 0,
            2 => cap.reference.parameter_hash_bytes = 0,
            3 => cap.reference.trace_hash_bytes = 0,
            4 => cap.topk_scan_work = 0,
            5 => cap.topk_auxiliary_bytes = 0,
            _ => cap.reference.attempts = 0,
        }
        let callbacks = Cell::new(0);
        let mut session = FlatReplaySessionV7::new(cap);
        let result = session.check(
            &t.context(),
            &t.envelope,
            &public,
            &|i, l| {
                callbacks.set(callbacks.get() + 1);
                t.params.tensors.get(&(i, l)).cloned()
            },
            &t.prompt,
        );
        assert!(matches!(result, FlatReplayResultV7::Limit(_)));
        assert_eq!(public.calls.get(), 0);
        assert_eq!(callbacks.get(), 0);
    }
    let mut cheap = t.envelope.clone();
    cheap.topk_scan_work_per_attempt -= 1;
    let public = t.public();
    let mut session = FlatReplaySessionV7::new(limits(&t.envelope, 1));
    assert!(matches!(
        session.check(&t.context(), &cheap, &public, &|_, _| panic!("no callback"), &t.prompt),
        FlatReplayResultV7::Inconsistent(_)
    ));
    assert_eq!(session.attempts, 0);
    assert_eq!(public.calls.get(), 0);
    let mut public = t.public();
    public.mode = 3;
    assert!(matches!(t.run(&public, &mut session), FlatReplayResultV7::Finding(FlatFindingV7::ValueDemand(_))));
    let calls = public.calls.get();
    let reads = public.values.get();
    assert_eq!(session.parameter_hash_bytes, t.envelope.reference.parameter_hash_bytes_per_attempt);
    assert_eq!(session.trace_hash_bytes, t.envelope.reference.trace_hash_bytes_per_attempt);
    assert_eq!(session.topk_scan_work, t.envelope.topk_scan_work_per_attempt);
    assert_eq!(t.run(&public, &mut session), FlatReplayResultV7::Limit("attempts"));
    assert_eq!(public.calls.get(), calls);
    assert_eq!(public.values.get(), reads);
    let mut cap = limits(&t.envelope, 1);
    cap.selected_value_reads = 0;
    let public = t.public();
    let mut session = FlatReplaySessionV7::new(cap);
    assert_eq!(t.run(&public, &mut session), FlatReplayResultV7::Limit("selected value reads"));
    assert_eq!(public.values.get(), 0);
}
#[test]
fn every_history_threshold_and_tail_uses_the_admitted_point_court_and_prior_state() {
    let p = history();
    let mut t = Trial::new(p, MapParams::default(), 130);
    let mut session = FlatReplaySessionV7::new(limits(&t.envelope, 1));
    assert_eq!(t.run(&t.public(), &mut session), FlatReplayResultV7::Finding(FlatFindingV7::Clean));
    for q in [0, 62, 63, 126, 128] {
        t.trace.values[q][0][3].data[0] = 1;
        t.refresh();
        let FlatFaultV7::TopK(f) = t.fault() else { panic!("wrong court") };
        assert_eq!(f.output.node.as_ref().unwrap().position, q as u32);
        assert_eq!(f.row, 0);
        t.trace.values[q][0][3].data[0] = 0;
        t.refresh();
    }
}
#[test]
fn initial_zero_and_per_layer_state_writers_select_courts_from_exact_wiring() {
    let (p, params) = state_topk();
    let mut t = Trial::new(p, params, 2);
    let relation = t.scope.relations.iter().find(|r| r.block == 1 && r.node == 0).unwrap();
    assert_eq!(relation.court, FlatRelationCourtV7::TopKMixedV7);
    assert_eq!(relation.occurrences, 3);
    let mut session = FlatReplaySessionV7::new(limits(&t.envelope, 1));
    assert_eq!(t.run(&t.public(), &mut session), FlatReplayResultV7::Finding(FlatFindingV7::Clean));
    for (p, s, ids, point) in
        [(0, 1, vec![0, 2], false), (0, 2, vec![0, 2], false), (1, 1, vec![0, 63], true), (1, 2, vec![0, 63], true)]
    {
        let old = t.trace.values[p][s][0].data.clone();
        t.trace.values[p][s][0].data = ids;
        t.refresh();
        let f = t.fault();
        assert_eq!(matches!(f, FlatFaultV7::TopK(_)), point);
        if point {
            let FlatFindingV5::Fault(old) =
                check_flat_claim_v5(&t.context().base, &t.public(), &|i, l| t.params.tensors.get(&(i, l)).cloned(), &t.prompt)
            else {
                panic!("legacy local proof")
            };
            assert!(verify_flat_fault_v7(&t.context(), &FlatFaultV7::V5(old)).is_err());
        }
        t.trace.values[p][s][0].data = old;
        t.refresh();
    }
}
#[test]
fn delivered_decode_fault_is_included_in_the_same_scope_and_mixed_codec() {
    let mut t = Trial::new(sparse(3, 1), sparse_params(3), 1);
    t.generated[0] = (t.generated[0] + 1) % 3;
    assert!(matches!(t.fault(), FlatFaultV7::V5(FlatFaultV5::Decode(_))));
}

#[test]
fn ordinary_encoder_graph_keeps_job_bound_inputs_and_exact_single_position_scope() {
    use misaka_palw_kernel::seg_encoder::{ENCODER_COUNT_PARAM_V1, ENCODER_IDS_PARAM_V1, encoder_binding_v1};
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let ids = b.param(ENCODER_IDS_PARAM_V1, DType::Idx, &[65], false);
    let count = b.param(ENCODER_COUNT_PARAM_V1, DType::Idx, &[], false);
    let mut pre = b.block("pre", vec![]);
    let y = pre.add(ids, count, DType::I32);
    let pre = pre.finish(&[y]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[65])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    let binding = encoder_binding_v1(&p).unwrap();
    let model = MapParams { tensors: [((0, None), binding.ids(&[0]).unwrap()), ((1, None), binding.count(1).unwrap())].into() };
    let trace = trace_v1(&p, &model, &[0]).unwrap();
    let scope = flat_resource_scope_v7(&p, 1, Some(&binding)).unwrap();
    assert!(scope.relations.iter().all(|r| r.court == FlatRelationCourtV7::V5));
    assert_eq!(scope.topk_selection_work_per_claim, 0);
    assert!(flat_resource_scope_v7(&p, 2, Some(&binding)).is_err());
    let mut wrong = binding;
    wrong.l += 1;
    assert!(flat_resource_scope_v7(&p, 1, Some(&wrong)).is_err());
    let envelope = flat_replay_envelope_v7(&p, &scope, Some(&binding)).unwrap();
    let mut trial = Trial {
        p,
        params: MapParams::default(),
        pc: ParamCommitmentsV1::default(),
        trace,
        prompt: vec![0],
        generated: vec![],
        commitments: SegmentedCommitmentsV1::new(vec![]),
        roots: vec![],
        scope,
        envelope,
    };
    trial.refresh();
    let mut context = trial.context();
    context.base.claim.encoder = Some(binding);
    let mut session = FlatReplaySessionV7::new(limits(&trial.envelope, 1));
    assert_eq!(
        session.check(&context, &trial.envelope, &trial.public(), &|_, _| panic!("public encoder inputs need no artifact read"), &[0]),
        FlatReplayResultV7::Finding(FlatFindingV7::Clean)
    );
    let mut cheap = trial.scope.clone();
    cheap.relations.clear();
    context.scope = &cheap;
    let public = trial.public();
    assert!(matches!(
        session.check(&context, &trial.envelope, &public, &|_, _| panic!("no callback"), &[0]),
        FlatReplayResultV7::Inconsistent(_)
    ));
    assert_eq!(public.calls.get(), 0);
}
