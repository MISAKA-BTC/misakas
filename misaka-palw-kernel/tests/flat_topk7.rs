//! Full semantic counterexample coverage and exact root/coordinate/codec refusal.
//! Synthetic parameter/node material, not a checkpoint or native acceptance.
use misaka_palw_kernel::{
    element::SegClaimContextV1,
    flat_court5::{FlatOperandV5, FlatSegContextV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
    flat_topk7::*,
    hash::Digest,
    job::DecodeRuleV1,
    merkle::AXIS_ROW,
    merkle3::{LayoutV3, LeafOpeningV3, TreesV3},
    merkle5::{FlatLeafValueV5, FlatTreeV5},
    seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1},
    trace::{ParamCommitmentsV1, TraceV1, trace_v1},
    verify::DismissalV1,
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TirProgramV1, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};
use std::collections::BTreeSet;
struct Fixture {
    p: TirProgramV1,
    params: MapParams,
    pc: ParamCommitmentsV1,
    trace: TraceV1,
    commit: SegmentedCommitmentsV1,
    roots: Vec<Digest>,
    top: u16,
    node_source: bool,
    axis: usize,
    k: u32,
}
impl Fixture {
    fn new(shape: &[u32], axis: usize, k: u32, data: Vec<i128>, node_source: bool) -> Self {
        Self::new_typed(shape, axis, k, if node_source { DType::I64 } else { DType::I32 }, data, node_source)
    }
    fn new_typed(shape: &[u32], axis: usize, k: u32, dtype: DType, data: Vec<i128>, node_source: bool) -> Self {
        let mut b = ProgramBuilder::new(128, HISTORY_BOUND_V1_SMALL);
        assert!(node_source || dtype != DType::I128);
        let param_dtype = if dtype == DType::I128 { DType::I32 } else { dtype };
        let scores = b.param("independent.scores", param_dtype, shape, false);
        let (pre, top) = {
            let mut block = b.block("pre", vec![]);
            let x = if node_source { block.cast(scores, dtype) } else { scores };
            let y = block.topk(x, axis, k);
            let Ref::Node(top) = y else { panic!() };
            (block.finish(&[y]), top)
        };
        let carry = vec![b.blocks[pre as usize].nodes[top as usize].out.clone()];
        let (post, logits) = {
            let mut block = b.block("post", carry);
            let y = block.cast(Ref::CarryIn(0), DType::I32);
            block.commit(y);
            let Ref::Node(n) = y else { panic!() };
            (block.finish(&[]), n)
        };
        let p = b.finish(pre, vec![], post, logits);
        TirProgramV1::decode_canonical(&p.encode()).unwrap();
        let params = MapParams {
            tensors: [((0, None), Tensor::new(param_dtype, shape.iter().map(|x| *x as usize).collect(), data).unwrap())].into(),
        };
        let pc = ParamCommitmentsV1::of_v3(&params);
        let trace = trace_v1(&p, &params, &[0]).unwrap();
        let mut f =
            Self { p, params, pc, trace, commit: SegmentedCommitmentsV1::new(vec![]), roots: vec![], top, node_source, axis, k };
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
    fn context(&self) -> TopKContextV7<'_> {
        TopKContextV7 {
            court_id: topk_court_id_v7(),
            base: FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &self.p,
                    params: &self.pc,
                    segment_roots: &self.roots,
                    positions: 1,
                    prompt_len: 1,
                    prompt_root: prompt_root_of_ids_v1(&[0]),
                    inline_prompt: Some(&[0]),
                    generated: &[0],
                    decode: DecodeRuleV1::Greedy,
                    encoder: None,
                },
            },
        }
    }
    fn selected(&self) -> &Tensor {
        &self.trace.values[0][0][self.top as usize]
    }
    fn scores(&self) -> &Tensor {
        if self.node_source { &self.trace.values[0][0][0] } else { &self.params.tensors[&(0, None)] }
    }
    fn at(&self, shape: &[usize], row: u64, slot: u32) -> u64 {
        let suffix = shape[self.axis + 1..].iter().product::<usize>() as u64;
        ((row / suffix) * shape[self.axis] as u64 + slot as u64) * suffix + row % suffix
    }
    fn set_row(&mut self, row: u64, ids: &[u32]) {
        for (slot, id) in ids.iter().enumerate() {
            let e = self.at(&self.selected().shape, row, slot as u32);
            self.trace.values[0][0][self.top as usize].data[e as usize] = *id as i128;
        }
        self.refresh();
    }
    fn selected_row(&self, row: u64) -> Vec<u32> {
        (0..self.k).map(|j| self.selected().data[self.at(&self.selected().shape, row, j) as usize] as u32).collect()
    }
    fn detect(&self, row: u64) -> Option<TopKFaultKindV7> {
        let shape = &self.scores().shape;
        let n = shape[self.axis] as u32;
        let mut budget = TopKScanBudgetV7 { remaining_work: u128::MAX, auxiliary_ceiling: TOPK_SCAN_MAX_AUX_BYTES_V7 };
        detect_topk_line_v7(n, self.k, &self.selected_row(row), &mut budget, |i| {
            Some(self.scores().data[self.at(shape, row, i) as usize])
        })
        .unwrap()
    }
    fn proof(&self, row: u64, kind: TopKFaultKindV7) -> TopKFaultV7 {
        let mut slots = Vec::new();
        let mut inputs = Vec::new();
        match kind {
            TopKFaultKindV7::Range { slot } => slots.push(slot),
            TopKFaultKindV7::Order { left } => slots.extend([left, left + 1]),
            TopKFaultKindV7::Dominance { selected, rival, gap } => {
                slots.push(selected);
                if gap > 0 {
                    slots.push(gap - 1)
                }
                if gap < self.k {
                    slots.push(gap)
                }
                let id = self.selected().data[self.at(&self.selected().shape, row, selected) as usize] as u32;
                inputs.extend([self.at(&self.scores().shape, row, id), self.at(&self.scores().shape, row, rival)]);
            }
        }
        let output =
            self.opening(self.selected(), true, slots.iter().map(|s| self.at(&self.selected().shape, row, *s)).collect(), self.top);
        let scores =
            if inputs.is_empty() { FlatOperandV5::default() } else { self.opening(self.scores(), self.node_source, inputs, 0) };
        TopKFaultV7 { version: 7, row, kind, output, scores }
    }
    fn opening(&self, t: &Tensor, node: bool, elements: Vec<u64>, n: u16) -> FlatOperandV5 {
        let opening = node.then(|| self.commit.node_opening(0, 0, n).unwrap());
        if node && t.data.len() >= 64 {
            let tree = FlatTreeV5::of(t).unwrap();
            let ids: BTreeSet<_> = elements.into_iter().map(|e| tree.layout().leaf_of(e).unwrap()).collect();
            FlatOperandV5 {
                node: opening,
                flat: ids.into_iter().map(|i| FlatLeafValueV5::of(t, &tree, i).unwrap()).collect(),
                legacy: vec![],
            }
        } else {
            let tree = TreesV3::of(t);
            let l = LayoutV3::of(&t.shape);
            let ids: BTreeSet<_> = elements
                .into_iter()
                .map(|e| {
                    let (line, tile, _) = l.row_leaf_of(e).unwrap();
                    (line, tile)
                })
                .collect();
            FlatOperandV5 {
                node: opening,
                flat: vec![],
                legacy: ids.into_iter().map(|(line, tile)| LeafOpeningV3::of_trees(t, &tree, AXIS_ROW, line, tile).unwrap()).collect(),
            }
        }
    }
}
#[test]
fn every_small_wrong_set_has_a_bounded_authentic_counterexample_and_honest_sets_do_not() {
    let mut tested = 0;
    for pattern in 0..27 {
        let scores = vec![(pattern % 3) as i128 - 1, ((pattern / 3) % 3) as i128 - 1, ((pattern / 9) % 3) as i128 - 1];
        for k in 1..=3 {
            let mut f = Fixture::new(&[3], 0, k, scores.clone(), false);
            let honest = f.selected_row(0);
            for word in 0..4u32.pow(k) {
                let mut x = word;
                let ids: Vec<_> = (0..k)
                    .map(|_| {
                        let v = x % 4;
                        x /= 4;
                        v
                    })
                    .collect();
                f.set_row(0, &ids);
                if let Some(kind) = f.detect(0) {
                    assert_ne!(ids, honest);
                    let proof = f.proof(0, kind);
                    let bytes = proof.to_bytes().unwrap();
                    let decoded = TopKFaultV7::from_bytes(&bytes).unwrap();
                    assert_eq!(decoded, proof);
                    assert_eq!(verify_topk_fault_v7(&f.context(), &decoded), Ok(()));
                } else {
                    assert_eq!(ids, honest);
                    let innocent = f.proof(0, TopKFaultKindV7::Range { slot: 0 });
                    assert_eq!(verify_topk_fault_v7(&f.context(), &innocent), Err(DismissalV1::NoFault));
                }
                tested += 1;
            }
        }
    }
    assert_eq!(tested, 2268);
}
#[test]
fn all_axes_rank_and_flat_or_legacy_output_grammars_keep_rows_and_gap_members_bound() {
    for (shape, axis, k) in [(vec![2, 130, 3], 1, 2), (vec![130, 2, 3], 0, 16), (vec![2, 3, 130], 2, 16), (vec![2, 3, 130, 2], 2, 2)] {
        for node in [false, true] {
            let count = shape.iter().product::<u32>() as usize;
            let data = (0..count).map(|i| ((i * 7919) % 127) as i128 - 63).collect();
            let mut f = Fixture::new(&shape, axis, k, data, node);
            let rows = count as u64 / shape[axis] as u64;
            for row in [0, rows - 1] {
                let honest = f.selected_row(row);
                assert!(f.detect(row).is_none());
                let mut wrong = honest.clone();
                wrong[0] = shape[axis];
                f.set_row(row, &wrong);
                let proof = f.proof(row, f.detect(row).unwrap());
                assert_eq!(verify_topk_fault_v7(&f.context(), &proof), Ok(()));
                let mut forged = proof.clone();
                forged.row = rows;
                assert!(verify_topk_fault_v7(&f.context(), &forged).is_err());
                f.set_row(row, &honest);
                let mut wrong = honest;
                wrong[0] = 0;
                if wrong[0] == wrong[1] {
                    wrong[0] = 1;
                }
                f.set_row(row, &wrong);
                if let Some(kind) = f.detect(row) {
                    let proof = f.proof(row, kind);
                    assert_eq!(verify_topk_fault_v7(&f.context(), &proof), Ok(()));
                }
            }
        }
    }
}
#[test]
fn borrowed_roots_members_scores_contexts_and_unbounded_codecs_never_convict() {
    let mut f = Fixture::new(&[130], 0, 2, vec![0; 130], true);
    f.set_row(0, &[2, 3]);
    let proof = f.proof(0, f.detect(0).unwrap());
    assert_eq!(verify_topk_fault_v7(&f.context(), &proof), Ok(()));
    let bytes = proof.to_bytes().unwrap();
    for cut in 0..bytes.len() {
        assert!(TopKFaultV7::from_bytes(&bytes[..cut]).is_err());
    }
    let mut suffix = bytes.clone();
    suffix.push(0);
    assert!(TopKFaultV7::from_bytes(&suffix).is_err());
    let mut old = bytes.clone();
    old[0] = 5;
    assert!(TopKFaultV7::from_bytes(&old).is_err());
    assert!(misaka_palw_kernel::flat_court5::FlatFaultV5::from_bytes(&bytes).is_err());
    for change in 0..8 {
        let mut bad = proof.clone();
        match change {
            0 => bad.output.node.as_mut().unwrap().commitment[0] ^= 1,
            1 => bad.output.node.as_mut().unwrap().position = 1,
            2 => bad.scores.node.as_mut().unwrap().node = f.top,
            3 => bad.scores.flat[0].bytes[0] ^= 1,
            4 => bad.output.legacy[0].values[0] = 0,
            5 => bad.row = 1,
            6 => bad.kind = TopKFaultKindV7::Dominance { selected: 0, rival: 2, gap: 0 },
            _ => bad.kind = TopKFaultKindV7::Dominance { selected: 0, rival: 0, gap: 1 },
        };
        assert!(verify_topk_fault_v7(&f.context(), &bad).is_err());
    }
    let mut context = f.context();
    context.court_id = [1; 64];
    assert!(verify_topk_fault_v7(&context, &proof).is_err());
    let mut context = f.context();
    context.base.profile = [1; 64];
    assert!(verify_topk_fault_v7(&context, &proof).is_err());
    let mut hostile = proof.clone();
    hostile.output.legacy.resize(4, hostile.output.legacy[0].clone());
    assert!(hostile.to_bytes().is_err());
    let raw = borsh::to_vec(&hostile).unwrap();
    assert!(TopKFaultV7::from_bytes(&raw).is_err());
    let scope = topk_resource_scope_v7(&f.context(), 0, 0, f.top).unwrap();
    assert!(check_topk_resource_scope_v7(&f.context(), &scope).is_ok());
    let mut cheap = scope;
    cheap.court_work -= 1;
    assert!(check_topk_resource_scope_v7(&f.context(), &cheap).is_err());
}
#[test]
fn scan_limits_precede_callbacks_and_failed_acquisition_spends_the_same_session() {
    let mut calls = 0;
    let mut budget = TopKScanBudgetV7 { remaining_work: 0, auxiliary_ceiling: TOPK_SCAN_MAX_AUX_BYTES_V7 };
    assert!(
        detect_topk_line_v7(100, 2, &[0, 1], &mut budget, |_| {
            calls += 1;
            Some(0)
        })
        .is_err()
    );
    assert_eq!(calls, 0);
    budget.remaining_work = u128::MAX;
    budget.auxiliary_ceiling = 127;
    assert!(
        detect_topk_line_v7(100, 2, &[0, 1], &mut budget, |_| {
            calls += 1;
            Some(0)
        })
        .is_err()
    );
    assert_eq!(calls, 0);
    budget.auxiliary_ceiling = 128;
    let before = budget.remaining_work;
    assert!(
        detect_topk_line_v7(100, 2, &[0, 1], &mut budget, |_| {
            calls += 1;
            None
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    let charged = before - budget.remaining_work;
    assert!(charged > 0);
    budget.remaining_work = charged - 1;
    assert!(
        detect_topk_line_v7(100, 2, &[0, 1], &mut budget, |_| {
            calls += 1;
            Some(0)
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    let mut budget = TopKScanBudgetV7 { remaining_work: u128::MAX, auxiliary_ceiling: u128::MAX };
    assert!(detect_topk_line_v7(u32::MAX, 1, &[0], &mut budget, |_| panic!("no reads")).is_err());
    assert!(detect_topk_line_v7(1 << 24, 1 << 24, &[], &mut budget, |_| panic!("no reads")).is_err());
}
#[test]
fn all_legal_parameter_and_internal_score_widths_bind_without_relaxing_the_frozen_type_rules() {
    let n = 12_288u32;
    for dtype in DType::ALL {
        for node in [false, true] {
            if !node && dtype == DType::I128 {
                continue; // Only i128 parameters are forbidden by frozen v1.
            }
            let mut data = vec![0; n as usize];
            data[0] = if dtype == DType::I128 { DType::I32.min_value() } else { dtype.min_value() };
            data[n as usize - 2] = if dtype == DType::I128 { DType::I32.max_value() } else { dtype.max_value() };
            let mut f = Fixture::new_typed(&[n], 0, 2, dtype, data, node);
            assert_eq!(f.scores().dtype, dtype);
            assert!(f.detect(0).is_none());
            f.set_row(0, &[0, 1]);
            let proof = f.proof(0, f.detect(0).unwrap());
            let bytes = proof.to_bytes().unwrap();
            let scope = topk_resource_scope_v7(&f.context(), 0, 0, f.top).unwrap();
            assert!(bytes.len() as u64 <= scope.filing_bytes);
            assert_eq!(verify_topk_fault_v7(&f.context(), &TopKFaultV7::from_bytes(&bytes).unwrap()), Ok(()));
            if dtype == DType::I128 {
                f.p.params[0].dtype = dtype;
                assert!(TirProgramV1::decode_canonical(&f.p.encode()).is_err());
                assert!(topk_resource_scope_v7(&f.context(), 0, 0, f.top).is_err());
                assert!(verify_topk_fault_v7(&f.context(), &proof).is_err());
            }
        }
    }
}
#[test]
fn actual_ten_million_axis_has_a_small_terminal_proof_and_independently_priced_scan() {
    let n = 10_485_760u32;
    let mut data = vec![0; n as usize];
    data[n as usize - 1] = 1;
    let mut f = Fixture::new(&[n], 0, 2, data, false);
    assert_eq!(f.selected_row(0), vec![0, n - 1]);
    assert!(
        misaka_palw_kernel::flat_scope5::flat_resource_scope_v5(&f.p, 1, None).is_err(),
        "frozen whole-axis scope must retain its rejection"
    );
    let scope = topk_resource_scope_v7(&f.context(), 0, 0, f.top).unwrap();
    assert_eq!(scope.axis_len, n);
    assert_eq!(scope.selection_aux_bytes, 128);
    assert!(scope.selection_work > n as u128);
    f.set_row(0, &[0, n - 2]);
    let proof = f.proof(0, f.detect(0).unwrap());
    let bytes = proof.to_bytes().unwrap();
    assert!(bytes.len() as u64 <= scope.filing_bytes);
    assert!(bytes.len() < 32 << 10);
    assert_eq!(verify_topk_fault_v7(&f.context(), &TopKFaultV7::from_bytes(&bytes).unwrap()), Ok(()));
    println!(
        "TopK-v7 synthetic actual axis={n} K=2 source_bytes={} filing_bytes={} court_bound={} court_work={} scratch_bound={} scan_work={} selection_aux={}; no checkpoint, dynamic-H, native or Final acceptance",
        n as u64 * 4,
        bytes.len(),
        scope.filing_bytes,
        scope.court_work,
        scope.court_scratch_bytes,
        scope.selection_work,
        scope.selection_aux_bytes
    );
}
