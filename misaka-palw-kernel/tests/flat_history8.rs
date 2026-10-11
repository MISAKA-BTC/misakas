//! Dynamic history-selection relation components, not a native/model task.
use misaka_palw_kernel::{
    flat_court5::{FlatOperandV5, flat_profile_commitment_v5},
    flat_detect5::{FlatPublicReadsV5, FlatReadV5},
    flat_detect7::{FlatPublicReadsV7, FlatValueReadV7, FlatValueV7, TOPK_VALUE_MAX_BYTES_V7},
    flat_history8::*,
    flat_topk7::{TopKFaultKindV7, TopKFaultV7},
    hash::Digest,
    merkle3::{LeafOpeningV3, TreesV3},
    merkle5::{FlatLeafHashV5, FlatLeafValueV5, FlatTreeV5},
    seg::{NodeOpeningV1, SegmentedCommitmentsV1},
    verify::DismissalV1,
};
use misaka_palw_tir::{DType, Tensor};
use std::cell::Cell;

fn plan(dtype: DType, window: u32, k: u32, positions: u32) -> HistorySelectionPlanV8 {
    HistorySelectionPlanV8 {
        version: 8,
        graph_root: [23; 64],
        source: HistoryNodeV8 { occurrence: 0, node: 0, slot: 0 },
        output: HistoryNodeV8 { occurrence: 0, node: 1, slot: 1 },
        nodes_per_position: 2,
        dtype,
        row_shape: vec![2],
        window,
        selection_cap: k,
        max_positions: positions,
    }
}
fn limits(scope: &HistorySelectionScopeV8) -> HistorySelectionExecutionLimitsV8 {
    HistorySelectionExecutionLimitsV8 {
        source_reference_bytes: scope.source_reference_bytes,
        output_reference_bytes: scope.output_reference_bytes,
        selection_work: scope.selection_work_per_claim,
        auxiliary_bytes: scope.selection_auxiliary_bytes,
    }
}
struct Trial {
    plan: HistorySelectionPlanV8,
    job: HistorySelectionJobV8,
    scope: HistorySelectionScopeV8,
    source: Vec<Tensor>,
    output: Vec<Tensor>,
    commitments: SegmentedCommitmentsV1,
    roots: Vec<Digest>,
    calls: Cell<usize>,
    mode: Cell<u8>,
}
impl Trial {
    fn new(dtype: DType, window: u32, k: u32, positions: u32) -> Self {
        let plan = plan(dtype, window, k, positions);
        let scope = plan.scope().unwrap();
        let source: Vec<_> = (0..positions)
            .map(|p| {
                let h = (p + 1).min(window);
                let mut values = Vec::new();
                for q in p + 1 - h..=p {
                    values.extend_from_slice(&[10, (q % 56) as i128]);
                }
                Tensor::new(dtype, vec![h as usize, 2], values).unwrap()
            })
            .collect();
        let output = source
            .iter()
            .enumerate()
            .map(|(p, s)| evaluate_history_selection_v8(&plan, p as u32, s, &limits(&scope)).unwrap())
            .collect();
        let job = HistorySelectionJobV8 { version: 8, plan_root: plan.root(), input_root: [31; 64], positions, nonce: [17; 64] };
        let mut t = Self {
            plan,
            job,
            scope,
            source,
            output,
            commitments: SegmentedCommitmentsV1::new(vec![]),
            roots: vec![],
            calls: Cell::new(0),
            mode: Cell::new(0),
        };
        t.refresh();
        t
    }
    fn refresh(&mut self) {
        self.commitments = SegmentedCommitmentsV1::new(
            self.source
                .iter()
                .zip(&self.output)
                .map(|(s, o)| vec![vec![flat_profile_commitment_v5(s).unwrap(), flat_profile_commitment_v5(o).unwrap()]])
                .collect(),
        );
        self.roots = self.commitments.segment_roots();
    }
    fn context(&self) -> HistorySelectionContextV8<'_> {
        HistorySelectionContextV8 { plan: &self.plan, job: &self.job, scope: &self.scope, segment_roots: &self.roots }
    }
    fn budget(&self, attempts: u32) -> HistorySelectionBudgetV8 {
        HistorySelectionBudgetV8 {
            attempts,
            selection_work: self.scope.selection_work_per_claim * attempts as u128,
            auxiliary_bytes: self.scope.selection_auxiliary_bytes,
            selected_reads: self.scope.selected_reads_per_claim * attempts as u128,
            reply_bytes: self.scope.selected_reply_bytes_per_claim * attempts as u128,
            source_hash_bytes: self.scope.source_hash_bytes_per_attempt * attempts as u128,
            source_tree_bytes: self.scope.source_tree_bytes,
            selected_auth_hash_bytes: 4 * self.scope.selected_reply_bytes_per_claim * attempts as u128,
        }
    }
    fn check(&self, p: u32, budget: &mut HistorySelectionBudgetV8) -> HistorySelectionFindingV8 {
        check_history_selection_v8(
            &self.context(),
            p,
            &self.source[p as usize],
            &self.commitments.node_opening(p, 0, 0).unwrap(),
            &self.commitments.node_opening(p, 0, 1).unwrap(),
            self,
            budget,
        )
    }
}
impl FlatPublicReadsV5 for Trial {
    fn node(&self, r: FlatReadV5) -> Option<NodeOpeningV1> {
        self.commitments.node_opening(r.position, r.occurrence, r.node)
    }
    fn hash(&self, _: FlatReadV5) -> Option<FlatLeafHashV5> {
        None
    }
}
impl FlatPublicReadsV7 for Trial {
    fn value(&self, r: FlatValueReadV7) -> Option<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        if self.mode.get() == 1 {
            return None;
        }
        if self.mode.get() == 2 {
            return Some(vec![0; TOPK_VALUE_MAX_BYTES_V7 as usize + 1]);
        }
        let p = r.node.position;
        let t = self.output.get(p as usize)?;
        let node = self.commitments.node_opening(p, r.node.occurrence, r.node.node)?;
        let mut value = FlatOperandV5 { node: Some(node), flat: vec![], legacy: vec![] };
        if t.data.len() >= 64 {
            let tree = FlatTreeV5::of(t).ok()?;
            value.flat.push(FlatLeafValueV5::of(t, &tree, tree.layout().leaf_of(r.element)?)?);
        } else {
            let tree = TreesV3::of(t);
            let (row, tile, _) = tree.layout.row_leaf_of(r.element)?;
            value.legacy.push(LeafOpeningV3::of_trees(t, &tree, 0, row, tile)?);
        }
        let mut bytes = FlatValueV7 { version: 7, value }.to_bytes().ok()?;
        if self.mode.get() == 3 {
            bytes[0] = 8;
        }
        Some(bytes)
    }
}

#[test]
fn min_k_h_is_exact_at_warmup_flat_thresholds_and_window_rollovers_for_all_dtypes() {
    for dtype in DType::ALL {
        let t = Trial::new(dtype, 128, 4, 130);
        for p in [0, 1, 2, 3, 31, 62, 63, 64, 126, 127, 128, 129] {
            let (source, selected) = t.plan.shapes(p).unwrap();
            assert_eq!(source[0], (p + 1).min(128) as usize);
            assert_eq!(selected[0], (p + 1).min(4) as usize);
            assert_eq!(t.output[p as usize].shape, selected);
            assert_eq!(t.plan.absolute_position(p, 0).unwrap(), p + 1 - source[0] as u32);
            assert_eq!(t.plan.absolute_position(p, source[0] as u32 - 1).unwrap(), p);
            assert!(t.plan.absolute_position(p, source[0] as u32).is_err());
            for slot in 0..selected[0] {
                assert_eq!(t.output[p as usize].data[slot * 2], slot as i128, "equal scores prefer the lower source index");
            }
            assert_eq!(t.check(p, &mut t.budget(1)), HistorySelectionFindingV8::Clean);
        }
    }
}

#[test]
fn range_duplicate_and_wrong_tie_selection_produce_bounded_independently_verified_v8_faults() {
    for kind in 0..3 {
        let mut t = Trial::new(DType::I32, 128, 4, 130);
        let p = 129;
        match kind {
            0 => t.output[p].data[0] = 128,
            1 => t.output[p].data[2] = t.output[p].data[0],
            _ => {
                for slot in 0..4 {
                    t.output[p].data[slot * 2] = (slot + 1) as i128;
                }
            }
        }
        t.refresh();
        let HistorySelectionFindingV8::Fault(f) = t.check(p as u32, &mut t.budget(1)) else {
            panic!("missing fault kind {kind}");
        };
        assert!(matches!(
            (kind, &f.kind),
            (0, TopKFaultKindV7::Range { .. }) | (1, TopKFaultKindV7::Order { .. }) | (2, TopKFaultKindV7::Dominance { .. })
        ));
        let bytes = f.to_bytes().unwrap();
        assert!(bytes.len() as u64 <= t.scope.filing_bytes);
        verify_history_selection_fault_v8(&t.context(), &HistorySelectionFaultV8::from_bytes(&bytes).unwrap()).unwrap();
        assert!(TopKFaultV7::from_bytes(&bytes).is_err(), "frozen v7 does not accept the new history-selection grammar");
        let mut borrowed = f.clone();
        borrowed.job_root[0] ^= 1;
        assert!(matches!(verify_history_selection_fault_v8(&t.context(), &borrowed), Err(DismissalV1::NotAuthentic(_))));
        let mut borrowed = f.clone();
        borrowed.plan_root[0] ^= 1;
        assert!(verify_history_selection_fault_v8(&t.context(), &borrowed).is_err());
        let mut borrowed = f.clone();
        borrowed.output.node.as_mut().unwrap().position -= 1;
        assert!(verify_history_selection_fault_v8(&t.context(), &borrowed).is_err());
        let mut borrowed = f;
        borrowed.row = 2;
        assert!(verify_history_selection_fault_v8(&t.context(), &borrowed).is_err());
    }
}

#[test]
fn prepaid_missing_oversized_and_wrong_version_replies_consume_attempts_and_never_pass() {
    let t = Trial::new(DType::I32, 128, 4, 130);
    for mode in 1..=3 {
        t.mode.set(mode);
        let mut b = t.budget(1);
        let reads = b.selected_reads;
        assert!(matches!(t.check(129, &mut b), HistorySelectionFindingV8::Demand(_)));
        assert_eq!(b.attempts, 0);
        assert_eq!(b.selected_reads, reads - 1);
        let called = t.calls.get();
        assert_eq!(t.check(129, &mut b), HistorySelectionFindingV8::Limit("attempts"));
        assert_eq!(t.calls.get(), called);
    }
    t.mode.set(0);
    for field in 0..7 {
        let mut b = t.budget(1);
        match field {
            0 => b.selection_work = 0,
            1 => b.auxiliary_bytes = 0,
            2 => b.source_hash_bytes = 0,
            3 => b.source_tree_bytes = 0,
            4 => b.selected_reads = 0,
            5 => b.reply_bytes = 0,
            _ => b.selected_auth_hash_bytes = 0,
        }
        let calls = t.calls.get();
        assert!(matches!(t.check(129, &mut b), HistorySelectionFindingV8::Limit(_)));
        assert_eq!(t.calls.get(), calls, "limit precedes dependent provider callback");
    }
}

#[test]
fn changed_history_or_row_coordinates_do_not_turn_source_continuity_faults_into_selection_convictions() {
    let t = Trial::new(DType::I32, 8, 4, 10);
    let mut reset = t.source[9].clone();
    reset.data.fill(0);
    let mut b = t.budget(1);
    let f = check_history_selection_v8(
        &t.context(),
        9,
        &reset,
        &t.commitments.node_opening(9, 0, 0).unwrap(),
        &t.commitments.node_opening(9, 0, 1).unwrap(),
        &t,
        &mut b,
    );
    assert!(matches!(f, HistorySelectionFindingV8::Inconsistent(_)));
    assert_eq!(b.attempts, 0);
    assert_eq!(t.calls.get(), 0);
    let borrowed = t.commitments.node_opening(8, 0, 0).unwrap();
    assert!(matches!(
        check_history_selection_v8(
            &t.context(),
            9,
            &t.source[9],
            &borrowed,
            &t.commitments.node_opening(9, 0, 1).unwrap(),
            &t,
            &mut t.budget(1)
        ),
        HistorySelectionFindingV8::Inconsistent(_)
    ));
    assert_eq!(t.calls.get(), 0);
    let mut changed = t.scope.clone();
    changed.selected_reply_bytes_per_claim -= 1;
    let c = HistorySelectionContextV8 { scope: &changed, ..t.context() };
    assert!(matches!(
        check_history_selection_v8(
            &c,
            9,
            &t.source[9],
            &t.commitments.node_opening(9, 0, 0).unwrap(),
            &t.commitments.node_opening(9, 0, 1).unwrap(),
            &t,
            &mut t.budget(1)
        ),
        HistorySelectionFindingV8::Inconsistent(_)
    ));
}

#[test]
fn long_context_metadata_is_bounded_without_allocating_positions_or_history_images() {
    let mut p = plan(DType::I32, HISTORY_SELECTION_MAX_POSITIONS_V8, 2, HISTORY_SELECTION_MAX_POSITIONS_V8);
    p.row_shape = vec![];
    let s = p.scope().unwrap();
    assert_eq!(s.max_history, 10_485_760);
    assert_eq!(s.max_selected, 2);
    assert_eq!(s.source_storage_bytes, 41_943_040);
    assert_eq!(s.source_reference_bytes, 167_772_160);
    assert_eq!(s.selection_auxiliary_bytes, 128);
    assert!(s.filing_bytes <= 128 << 10);
    for field in 0..9 {
        let mut bad = p.clone();
        match field {
            0 => bad.version = 1,
            1 => bad.max_positions += 1,
            2 => bad.window += 1,
            3 => bad.selection_cap = 0,
            4 => bad.selection_cap = 1_000_000,
            5 => bad.row_shape = vec![u32::MAX],
            6 => bad.row_shape = vec![64],
            7 => bad.source.slot = bad.output.slot,
            _ => bad.nodes_per_position = 0,
        }
        assert!(bad.scope().is_err(), "invalid metadata field {field}");
    }
    let mut job =
        HistorySelectionJobV8 { version: 8, plan_root: p.root(), input_root: [33; 64], positions: p.max_positions, nonce: [0; 64] };
    job.check(&p).unwrap();
    job.positions += 1;
    assert!(job.check(&p).is_err());
    job.positions = 1;
    job.plan_root[0] ^= 1;
    assert!(job.check(&p).is_err());
}

#[test]
fn strict_v8_plan_and_fault_codecs_reject_truncation_unknown_versions_counts_and_trailing_bytes() {
    let t = Trial::new(DType::I32, 8, 4, 10);
    let encoded = borsh::to_vec(&t.plan).unwrap();
    assert_eq!(HistorySelectionPlanV8::from_bytes(&encoded).unwrap(), t.plan);
    for n in 0..encoded.len() {
        assert!(HistorySelectionPlanV8::from_bytes(&encoded[..n]).is_err());
    }
    let mut bad = encoded.clone();
    bad[..2].copy_from_slice(&1u16.to_le_bytes());
    assert!(HistorySelectionPlanV8::from_bytes(&bad).is_err());
    let count_at = 2 + 64 + 2 * (2 + 2 + 8) + 8 + 1;
    let mut bad = encoded.clone();
    bad[count_at..count_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(HistorySelectionPlanV8::from_bytes(&bad).is_err());
    let mut bad = encoded;
    bad.push(0);
    assert!(HistorySelectionPlanV8::from_bytes(&bad).is_err());
    let mut wrong = Trial::new(DType::I32, 8, 4, 10);
    wrong.output[9].data[0] = 8;
    wrong.refresh();
    let HistorySelectionFindingV8::Fault(f) = wrong.check(9, &mut wrong.budget(1)) else {
        panic!("missing range fault");
    };
    let encoded = f.to_bytes().unwrap();
    for n in 0..encoded.len() {
        assert!(HistorySelectionFaultV8::from_bytes(&encoded[..n]).is_err());
    }
    let mut bad = encoded;
    bad.push(0);
    assert!(HistorySelectionFaultV8::from_bytes(&bad).is_err());
    assert!(HistorySelectionFaultV8::from_bytes(&vec![0; HISTORY_SELECTION_MAX_FAULT_BYTES_V8 as usize + 1]).is_err());
}

#[test]
fn exact_selection_handles_i128_extremes_and_refuses_work_memory_before_output_allocation() {
    let mut p = plan(DType::I128, 8, 3, 8);
    p.row_shape = vec![];
    let scope = p.scope().unwrap();
    let source = Tensor::new(DType::I128, vec![4], vec![i128::MIN, i128::MAX, i128::MAX, 0]).unwrap();
    let expected = evaluate_history_selection_v8(&p, 3, &source, &limits(&scope)).unwrap();
    assert_eq!(expected.data, vec![1, 2, 3]);
    for field in 0..4 {
        let mut l = limits(&scope);
        match field {
            0 => l.source_reference_bytes = 0,
            1 => l.output_reference_bytes = 0,
            2 => l.selection_work = 0,
            _ => l.auxiliary_bytes = 0,
        }
        assert!(evaluate_history_selection_v8(&p, 3, &source, &l).is_err());
    }
    let oversized = Tensor::new(DType::I128, vec![8], vec![0; 8]).unwrap();
    assert!(evaluate_history_selection_v8(&p, 3, &oversized, &limits(&scope)).is_err());
}

#[test]
fn every_small_dynamic_selection_tuple_is_clean_exactly_when_its_set_is_canonical() {
    let mut trials = 0;
    for cap in 1..=4 {
        for h in 1..=4 {
            let mut t = Trial::new(DType::I32, 4, cap, 4);
            let k = h.min(cap) as usize;
            let p = h as usize - 1;
            let base = h as usize + 1;
            for code in 0..base.pow(k as u32) {
                let mut value = code;
                let mut canonical = true;
                for slot in 0..k {
                    let id = value % base;
                    value /= base;
                    t.output[p].data[slot * 2] = id as i128;
                    canonical &= id == slot;
                }
                t.refresh();
                match t.check(p as u32, &mut t.budget(1)) {
                    HistorySelectionFindingV8::Clean => assert!(canonical),
                    HistorySelectionFindingV8::Fault(f) => {
                        assert!(!canonical);
                        verify_history_selection_fault_v8(&t.context(), &f).unwrap();
                    }
                    other => panic!("cap {cap}, H {h}, code {code}: {other:?}"),
                }
                trials += 1;
            }
        }
    }
    assert_eq!(trials, 966);
}
