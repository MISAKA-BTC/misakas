#![allow(warnings)]
use misaka_palw_kernel::{
    element::SegClaimContextV1,
    flat_court5::{FlatOperandV5, FlatSegContextV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
    flat_court7::{FlatContextV7, FlatFaultV7, verify_flat_fault_v7},
    flat_detect5::{FlatPublicReadsV5, FlatReadV5},
    flat_detect7::*,
    flat_scope7::flat_resource_scope_v7,
    flat_topk_locator21::*,
    flat_topk7::{TopKFaultKindV7, TopKScanBudgetV7},
    job::DecodeRuleV1,
    merkle3::{LayoutV3, LeafOpeningV3, TreesV3},
    merkle5::{FlatLeafHashV5, FlatLeafValueV5, FlatTreeV5},
    seg::{NodeOpeningV1, SegmentedCommitmentsV1, prompt_root_of_ids_v1},
    trace::{ParamCommitmentsV1, TraceV1, trace_v1},
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TirProgramV1, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};
use std::cell::Cell;
fn program(rows: u32, k: u32) -> TirProgramV1 {
    let mut b = ProgramBuilder::new(512, HISTORY_BOUND_V1_SMALL);
    let scores = b.param("independent.scores", DType::I32, &[rows, 65], false);
    let mut pre = b.block("pre", vec![]);
    let x = pre.cast(Ref::Input(0), DType::I32);
    pre.commit(x);
    let pre = pre.finish(&[]);
    let mut post = b.block("topk", vec![]);
    let ids = post.topk(scores, 1, k);
    let logits = post.cast(ids, DType::I32);
    post.commit(logits);
    let post = post.finish(&[]);
    b.finish(pre, vec![], post, 1)
}
struct Trial {
    p: TirProgramV1,
    params: MapParams,
    pc: ParamCommitmentsV1,
    own: TraceV1,
    public: TraceV1,
    c: SegmentedCommitmentsV1,
    roots: Vec<[u8; 64]>,
    prompt: Vec<u32>,
    generated: Vec<u32>,
    mode: u8,
    calls: Cell<u64>,
    values: Cell<u64>,
}
impl Trial {
    fn new(rows: u32, k: u32, bad: Option<(usize, i128)>) -> Self {
        let p = program(rows, k);
        let params = MapParams {
            tensors: [(
                (0, None),
                Tensor::new(DType::I32, vec![rows as usize, 65], (0..rows as usize * 65).map(|i| (i % 65) as i128).collect()).unwrap(),
            )]
            .into(),
        };
        let prompt = vec![1];
        let own = trace_v1(&p, &params, &prompt).unwrap();
        let mut public = own.clone();
        if let Some((e, v)) = bad {
            public.values[0][1][0].data[e] = v;
        }
        let c = SegmentedCommitmentsV1::new(
            public
                .values
                .iter()
                .map(|p| p.iter().map(|o| o.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                .collect(),
        );
        let roots = c.segment_roots();
        let pc = ParamCommitmentsV1::of_v3(&params);
        Self { p, params, pc, own, public, c, roots, prompt, generated: vec![0], mode: 0, calls: Cell::new(0), values: Cell::new(0) }
    }
    fn locate(&self) -> FlatFindingV7 {
        let scope = flat_resource_scope_v7(&self.p, 1, None).unwrap();
        let c = FlatContextV7 {
            scope: &scope,
            base: FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &self.p,
                    params: &self.pc,
                    segment_roots: &self.roots,
                    positions: 1,
                    prompt_len: 1,
                    prompt_root: prompt_root_of_ids_v1(&self.prompt),
                    inline_prompt: Some(&self.prompt),
                    generated: &self.generated,
                    decode: DecodeRuleV1::Greedy,
                    encoder: None,
                },
            },
        };
        let mut scan = TopKScanBudgetV7 { remaining_work: u128::MAX, auxiliary_ceiling: 32 << 20 };
        let f = locate_selected_topk_v21(
            &c,
            self,
            FlatReadV5 { position: 0, occurrence: 1, node: 0, leaf: None },
            &self.own.values[0][1][0],
            &|| Ok(vec![self.params.tensors[&(0, None)].clone()]),
            &mut scan,
        );
        if let FlatFindingV7::Fault(f) = &f {
            verify_flat_fault_v7(&c, f).unwrap();
            let b = f.to_bytes().unwrap();
            verify_flat_fault_v7(&c, &FlatFaultV7::from_bytes(&b).unwrap()).unwrap();
        }
        f
    }
}
impl FlatPublicReadsV5 for Trial {
    fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
        self.calls.set(self.calls.get() + 1);
        let mut n = self.c.node_opening(u.position, u.occurrence, u.node)?;
        if self.mode == 1 {
            n.position += 1;
        }
        Some(n)
    }
    fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
        self.calls.set(self.calls.get() + 1);
        let t = &self.public.values[u.position as usize][u.occurrence as usize][u.node as usize];
        let mut h = FlatTreeV5::of(t).ok()?.opening(u.leaf?)?;
        if self.mode == 2 {
            h.index += 1;
        }
        Some(h)
    }
}
impl FlatPublicReadsV7 for Trial {
    fn value(&self, r: FlatValueReadV7) -> Option<Vec<u8>> {
        self.values.set(self.values.get() + 1);
        if self.mode == 3 {
            return None;
        }
        if self.mode == 4 {
            return Some(vec![0; TOPK_VALUE_MAX_BYTES_V7 as usize + 1]);
        }
        let u = r.node;
        let t = &self.public.values[u.position as usize][u.occurrence as usize][u.node as usize];
        let mut out = FlatOperandV5 { node: Some(self.c.node_opening(u.position, u.occurrence, u.node)?), ..Default::default() };
        if t.data.len() >= 64 {
            let tree = FlatTreeV5::of(t).ok()?;
            out.flat.push(FlatLeafValueV5::of(t, &tree, tree.layout().leaf_of(r.element)?)?);
        } else {
            let (row, tile, _) = LayoutV3::of(&t.shape).row_leaf_of(r.element)?;
            out.legacy.push(LeafOpeningV3::of_trees(t, &TreesV3::of(t), 0, row, tile)?);
        }
        if self.mode == 5 {
            out.node.as_mut()?.commitment[0] ^= 1;
        }
        FlatValueV7 { version: 7, value: out }.to_bytes().ok()
    }
}
#[test]
fn first_and_last_unknown_bad_rows_are_bounded_without_scanning_all_rows() {
    for rows in [1, 2, 16, 32, 65] {
        for k in [1, 2, 4, 7] {
            for row in [0, rows / 2, rows - 1] {
                for slot in [0, k - 1] {
                    let e = (row * k + slot) as usize;
                    let t = Trial::new(rows, k, Some((e, 0)));
                    let FlatFindingV7::Fault(FlatFaultV7::TopK(f)) = t.locate() else {
                        panic!("no TopK fault{rows}/{k}/{row}/{slot}")
                    };
                    assert_eq!(f.row, row as u64);
                    let shape = &t.own.values[0][1][0].shape;
                    let bound = topk_relation_callback_bound_v21(shape, k).unwrap();
                    assert!(t.calls.get() + t.values.get() <= bound);
                    if rows * k >= 64 {
                        assert!(t.values.get() <= k as u64 + 4, "row count must not affect value queries");
                    }
                }
            }
        }
    }
}
#[test]
fn range_order_and_dominance_use_unchanged_frozen_courts() {
    for (slot, v, want) in [(0, u32::MAX as i128, 0), (1, 61, 1), (0, 0, 2)] {
        let t = Trial::new(32, 4, Some((31 * 4 + slot, v)));
        let FlatFindingV7::Fault(FlatFaultV7::TopK(f)) = t.locate() else { panic!("no point fault") };
        let kind = match f.kind {
            TopKFaultKindV7::Range { .. } => 0,
            TopKFaultKindV7::Order { .. } => 1,
            TopKFaultKindV7::Dominance { .. } => 2,
        };
        assert_eq!(kind, want);
    }
}
#[test]
fn malformed_missing_substituted_public_material_never_becomes_clean_or_a_conviction() {
    for mode in 1..=5 {
        let mut t = Trial::new(32, 4, Some((127, 0)));
        t.mode = mode;
        assert!(matches!(t.locate(), FlatFindingV7::Demand(_) | FlatFindingV7::ValueDemand(_)), "mode{mode}");
    }
    let t = Trial::new(32, 4, None);
    assert_eq!(t.locate(), FlatFindingV7::Clean);
    assert_eq!(t.calls.get(), 1);
    assert_eq!(t.values.get(), 0);
}
