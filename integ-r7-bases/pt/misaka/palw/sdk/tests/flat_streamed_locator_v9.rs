//! Bounded independent mapped execution and ordinary-only public acquisition.
//! Synthetic controls do not count as checkpoint/Final/population coverage.
use kaspa_consensus_core::palw_flat_model_vector_v9::FlatModelVectorViewV9;
use misaka_palw_kernel::{
    flat_court5::{FlatHeaderV5, FlatOperandV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
    flat_detect5::{FlatPublicReadsV5, FlatReadV5},
    flat_detect9::*,
    flat_replay9::*,
    flat_resources10::ResourceQuantitiesV10,
    flat_scope9::*,
    job::DecodeRuleV1,
    merkle3::{LeafOpeningV3, TreesV3},
    merkle5::{FlatLeafHashV5, FlatLeafValueV5, FlatTreeV5},
    seg::{NodeOpeningV1, SegmentedCommitmentsV1, prompt_root_of_ids_v1},
    seg_ledger::{JobViewV1, SegClaimViewV1},
    trace::{ParamCommitmentsV1, TraceV1, trace_v1},
};
use misaka_palw_sdk::flat_streamed_locator_v9 as streamed;
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};
use std::{cell::Cell, path::PathBuf};
use streamed::*;
struct Trial {
    view: FlatModelVectorViewV9,
    trace: TraceV1,
    public: SegmentedCommitmentsV1,
    path: PathBuf,
    unit: FlatReadV5,
}
impl Drop for Trial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn trial(case: u8) -> Trial {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
    let param = b.param("own", DType::I32, &[256], false);
    let mut pre = b.block("pre", vec![]);
    let x = if case == 3 { pre.topk(param, 0, 2) } else { pre.add(param, Ref::Input(0), DType::I64) };
    let x = if case == 3 { pre.cast(x, DType::I32) } else { pre.clamp(x, i32::MIN as i64, i32::MAX as i64, DType::I32) };
    let dtype = DType::I32;
    let size = if case == 3 { 2 } else { 256 };
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(dtype, &[size])]);
    let x = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(x);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    let params = MapParams {
        tensors: [((0, None), Tensor::new(DType::I32, vec![256], (0..256).map(|i| i as i128).collect()).unwrap())].into(),
    };
    let pc = ParamCommitmentsV1::of_v3(&params);
    let mut trace = trace_v1(&p, &params, &[0]).unwrap();
    let generated = DecodeRuleV1::Greedy.select(&trace.values[0][1][0]).unwrap();
    if case == 1 {
        trace.values[0][0][0].data[150] += 1;
    }
    if case == 3 {
        trace.values[0][0][0].data[0] = 0;
    }
    if case == 4 {
        trace.values[0][1][0].data[150] += 1;
    }
    let public = SegmentedCommitmentsV1::new(
        trace
            .values
            .iter()
            .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
            .collect(),
    );
    let scope = flat_resource_scope_v9(&p, 1, None).unwrap();
    let replay = flat_replay_envelope_v9(&p, &scope, None).unwrap();
    let view = FlatModelVectorViewV9 {
        profile: flat_trace_profile_id_v5(),
        scope,
        replay,
        trace: SegClaimViewV1 {
            program: p.clone(),
            params: pc,
            segment_roots: public.segment_roots(),
            positions: 1,
            job: JobViewV1 {
                class_binding_id: [7; 64],
                prompt_len: 1,
                prompt_root: prompt_root_of_ids_v1(&[0]),
                inline_prompt: Some(vec![0]),
                max_new_tokens: 1,
                decode: DecodeRuleV1::Greedy,
                complete: true,
            },
            generated: vec![if case == 2 { 0 } else { generated }],
            encoder: None,
        },
    };
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "mapped-ordinary9-{}-{}.palwtir",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    misaka_palw_tir_artifact::write_container_v1(&path, &p, vec![], [0; 64], "{}".into(), &mut |j, l| {
        Ok(params.tensors[&(j, l)].to_le_bytes())
    })
    .unwrap();
    Trial { view, trace, public, path, unit: FlatReadV5 { position: 0, occurrence: 0, node: 0, leaf: None } }
}
fn limits() -> MappedFlatReplayLimitsV9 {
    MappedFlatReplayLimitsV9 {
        reference: ResourceQuantitiesV10([1 << 40; 21]),
        retained_tensor_bytes: 1 << 20,
        raw_artifact_bytes: 1 << 20,
        parameter_workspace_bytes: 1 << 20,
        selected_parameter_bytes: 1 << 20,
        selected_parameter_total_bytes: 16 << 20,
        attempts: 32,
        public_reads: 128,
        public_reply_bytes: 2 << 20,
        scope_checker_work: 1 << 40,
        parameter_hash_bytes: 1 << 40,
        trace_hash_bytes: 1 << 40,
        artifact_hash_bytes: 2 << 20,
        inventory: MappedInventoryLimitsV9 {
            leaves: 1 << 20,
            raw_bytes: 1 << 20,
            hash_bytes: 4 << 20,
            work: 1 << 40,
            workspace_bytes: 128 << 10,
        },
    }
}
fn binding(t: &Trial) -> kaspa_consensus_core::palw_model_artifact_v2::ModelArtifactBindingHeaderV2 {
    use kaspa_consensus_core::{Hash64, palw_tir_artifact_v1::PalwTirModelInventoryV2};
    let descriptor = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor();
    let a = misaka_palw_tir_exec::node::TirArtifactV1::open(&t.path).unwrap();
    let inventory = PalwTirModelInventoryV2::new(&descriptor, &t.view.trace.program).unwrap();
    let src = misaka_palw_sdk::tir_stream::ContainerRanges::open_with_window(a.container(), 1).unwrap();
    let (root, _) = misaka_palw_sdk::tir_stream::palw_tir_model_inventory_root_streamed_v2(&inventory, &src).unwrap();
    kaspa_consensus_core::palw_model_artifact_v2::ModelArtifactBindingHeaderV2 {
        descriptor: descriptor.digest(),
        program_root: misaka_palw_kernel::public::program_root_v1(&t.view.trace.program.encode()),
        program_bytes_len: t.view.trace.program.encode().len() as u32,
        model_inventory_root: root,
        kernel_param_root: Hash64::from_bytes(t.view.trace.params.root()),
        pc_instances: t.view.trace.params.by_instance.len() as u32,
        max_proof_bytes: 1 << 20,
        binder: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::from_u64_word(1),
            0,
        )),
        bound_daa: 0,
        matures_daa: 1,
        final_daa: 100,
        reserved: 1,
        refuted: false,
    }
}
fn prepare(t: &Trial, path: &std::path::Path, limits: MappedFlatReplayLimitsV9) -> Result<MappedFlatReplayV9, String> {
    let h = binding(t);
    MappedFlatReplayV9::prepare(&t.view, &h, h.id(), path, limits)
}

struct Served<'a> {
    t: &'a Trial,
    calls: Cell<u32>,
    missing: bool,
    foreign: bool,
    header_missing: bool,
    value_missing: bool,
    hide_prefix: bool,
}
impl FlatPublicReadsV5 for Served<'_> {
    fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
        self.calls.set(self.calls.get() + 1);
        if self.missing || self.hide_prefix && u.occurrence == 0 {
            return None;
        }
        let mut o = self.t.public.node_opening(u.position, u.occurrence, u.node)?;
        if self.foreign {
            o.node = o.node.wrapping_add(1);
        }
        Some(o)
    }
    fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
        self.calls.set(self.calls.get() + 1);
        FlatTreeV5::of(&self.t.trace.values[u.position as usize][u.occurrence as usize][u.node as usize]).ok()?.opening(u.leaf?)
    }
}
impl FlatPublicReadsV9 for Served<'_> {
    fn header(&self, r: FlatHeaderReadV9) -> Option<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        if self.header_missing {
            return None;
        }
        let t = &self.t.trace.values[r.node.position as usize][r.node.occurrence as usize][r.node.node as usize];
        let shape = t.shape.iter().map(|n| *n as u64).collect();
        let header = if t.len() >= 64 {
            FlatHeaderV5::Flat { dtype: t.dtype.tag(), shape, root: FlatTreeV5::of(t).ok()?.root() }
        } else {
            let trees = TreesV3::of(t);
            FlatHeaderV5::Legacy { dtype: t.dtype.tag(), shape, row_root: trees.row_root(), col_root: trees.col_root() }
        };
        FlatHeaderValueV9 { version: 9, scope: r.scope, header }.to_bytes().ok()
    }
    fn value(&self, r: FlatValueReadV9) -> Option<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        if self.value_missing {
            return None;
        }
        let t = &self.t.trace.values[r.node.position as usize][r.node.occurrence as usize][r.node.node as usize];
        let mut value =
            FlatOperandV5 { node: self.t.public.node_opening(r.node.position, r.node.occurrence, r.node.node), ..Default::default() };
        if t.len() >= 64 {
            let tree = FlatTreeV5::of(t).ok()?;
            value.flat.push(FlatLeafValueV5::of(t, &tree, tree.layout().leaf_of(r.element)?)?);
        } else {
            let l = misaka_palw_kernel::merkle3::LayoutV3::of(&t.shape);
            let (row, tile, _) = l.row_leaf_of(r.element)?;
            value.legacy.push(LeafOpeningV3::of_trees(t, &TreesV3::of(t), 0, row, tile)?);
        }
        FlatValueV9 { version: 9, scope: r.scope, value }.to_bytes().ok()
    }
}
fn served(t: &Trial) -> Served<'_> {
    Served { t, calls: Cell::new(0), missing: false, foreign: false, header_missing: false, value_missing: false, hide_prefix: false }
}
#[test]
fn independent_mapped_ordinary_unknown_leaf_and_missing_typed_values() {
    let t = trial(1);
    let mut own = prepare(&t, &t.path, limits()).unwrap();
    assert_eq!(own.usage().authenticated_parameter_instances, 1);
    assert_eq!(own.usage().independently_executed_nodes, 3);
    let mut s = served(&t);
    s.header_missing = true;
    assert!(matches!(own.check(&t.view, &s), MappedFlatReplayResultV9::Finding(FlatFindingV9::HeaderDemand(_))));
    s.header_missing = false;
    s.value_missing = true;
    assert!(matches!(own.check(&t.view, &s), MappedFlatReplayResultV9::Finding(FlatFindingV9::ValueDemand(_))));
    s.value_missing = false;
    let MappedFlatReplayResultV9::Finding(FlatFindingV9::Fault(f)) = own.check(&t.view, &s) else {
        panic!("no independently generated fault")
    };
    let misaka_palw_kernel::flat_court9::FlatFaultV9::Ordinary(point) = &f else { panic!("wrong court") };
    assert_eq!(point.element, 150);
    assert!(point.output.flat[0].opening.index > 0);
    misaka_palw_kernel::flat_court9::verify_flat_fault_v9(&t.view.context(), &f).unwrap();
    assert!(own.usage().attempts >= 3 && own.usage().public_reads > 0 && own.usage().reserved_reply_bytes > 0);
}
#[test]
fn ordinary_clean_uses_complete_own_commitments_without_public_callback() {
    let t = trial(0);
    let mut own = prepare(&t, &t.path, limits()).unwrap();
    let s = served(&t);
    assert_eq!(own.check(&t.view, &s), MappedFlatReplayResultV9::Finding(FlatFindingV9::Clean));
    assert_eq!(s.calls.get(), 0);
    assert!(own.reference_node(t.unit).is_some());
    assert_eq!(own.reference_parameter(0, None).unwrap().data.len(), 256);
}
#[test]
fn refusal_precedes_missing_file_and_public_callback() {
    let t = trial(1);
    let mut l = limits();
    l.reference = ResourceQuantitiesV10::default();
    let e = prepare(&t, std::path::Path::new("/missing-model-no-callback"), l).err().unwrap();
    assert!(e.contains("reference10"));
    let mut l = limits();
    l.retained_tensor_bytes = 0;
    assert!(prepare(&t, std::path::Path::new("/missing-model-no-callback"), l).err().unwrap().contains("retained"));
    let mut l = limits();
    l.public_reads = 0;
    let mut own = prepare(&t, &t.path, l).unwrap();
    let s = served(&t);
    assert_eq!(own.check(&t.view, &s), MappedFlatReplayResultV9::Limit("public reads"));
    assert_eq!(s.calls.get(), 0);
}
#[test]
fn missing_borrowed_reply_and_retry_charge_never_grant_clean() {
    let t = trial(1);
    let mut l = limits();
    l.attempts = 1;
    let mut own = prepare(&t, &t.path, l).unwrap();
    let mut s = served(&t);
    s.foreign = true;
    assert_eq!(own.check(&t.view, &s), MappedFlatReplayResultV9::Finding(FlatFindingV9::Demand(t.unit)));
    let u = own.usage();
    assert_eq!(u.attempts, 1);
    assert_eq!(u.public_reads, 1);
    assert_eq!(u.reserved_reply_bytes, 8192);
    let before = s.calls.get();
    assert_eq!(own.check(&t.view, &s), MappedFlatReplayResultV9::Limit("attempts"));
    assert_eq!(s.calls.get(), before);
}
#[test]
fn subject_swap_is_refused_before_callback_and_source_identity_is_checked() {
    let mut t = trial(1);
    let mut own = prepare(&t, &t.path, limits()).unwrap();
    let old = t.view.trace.job.class_binding_id;
    t.view.trace.job.class_binding_id = [9; 64];
    let s = served(&t);
    assert!(matches!(own.check(&t.view, &s), MappedFlatReplayResultV9::Inconsistent(_)));
    assert_eq!(s.calls.get(), 0);
    t.view.trace.job.class_binding_id = old;
    t.view.trace.params.by_instance.insert((0, None), [0; 64]);
    assert!(prepare(&t, &t.path, limits()).err().unwrap().contains("PC instance"));
}
#[test]
fn topk_and_decode_are_special_courts_with_owned_reference_material() {
    let t = trial(3);
    let mut own = prepare(&t, &t.path, limits()).unwrap();
    let s = served(&t);
    assert_eq!(own.check(&t.view, &s), MappedFlatReplayResultV9::NeedsSpecial { unit: t.unit, reason: MappedSpecialCourtV9::TopK });
    let t = trial(2);
    let mut own = prepare(&t, &t.path, limits()).unwrap();
    let s = served(&t);
    assert!(matches!(
        own.check(&t.view, &s),
        MappedFlatReplayResultV9::NeedsSpecial { reason: MappedSpecialCourtV9::Decode { index: 0 }, .. }
    ));
    assert_eq!(s.calls.get(), 0);
}

#[test]
fn authenticated_prefix_reuses_exact_subject_but_job_inputs_still_authenticate() {
    let t = trial(4);
    let mut own = prepare(&t, &t.path, limits()).unwrap();
    let mut s = served(&t);
    s.header_missing = true;
    assert!(matches!(own.check(&t.view, &s), MappedFlatReplayResultV9::Finding(FlatFindingV9::HeaderDemand(_))));
    assert_eq!(own.authenticated_matching_prefix_nodes(), 2);
    s.hide_prefix = true;
    assert!(matches!(own.check(&t.view, &s), MappedFlatReplayResultV9::Finding(FlatFindingV9::HeaderDemand(_))));
    let mut t = trial(0);
    t.view.trace.job.inline_prompt = Some(vec![300]);
    assert!(prepare(&t, std::path::Path::new("/missing-before-job-check"), limits()).err().unwrap().contains("fixed-count"));
}

#[test]
fn source_preparation_and_public_wire_quota_are_paid_before_callback() {
    let t = trial(1);
    let mut l = limits();
    l.selected_parameter_total_bytes = 0;
    let mut own = prepare(&t, &t.path, l).unwrap();
    let s = served(&t);
    assert_eq!(own.check(&t.view, &s), MappedFlatReplayResultV9::Limit("selected parameter preparation bytes"));
    assert_eq!(own.usage().selected_parameter_bytes, 0);
    let before = s.calls.get();
    assert_eq!(own.check(&t.view, &s), MappedFlatReplayResultV9::Limit("selected parameter preparation bytes"));
    assert_eq!(s.calls.get(), before);
    let mut l = limits();
    l.public_reply_bytes = 8191;
    let mut own = prepare(&t, &t.path, l).unwrap();
    let s = served(&t);
    assert_eq!(own.check(&t.view, &s), MappedFlatReplayResultV9::Limit("public reply bytes"));
    assert_eq!(s.calls.get(), 0);
}

#[test]
fn complete_model_inventory_root_and_every_quota_precede_execution() {
    let t = trial(0);
    let h = binding(&t);
    let q = mapped_model_inventory_quote_v9(&t.view.trace.program).unwrap();
    assert_eq!(q.leaves, 1);
    assert_eq!(q.raw_bytes, 1024);
    assert_eq!(q.hash_bytes, 1024 + 16 + 3);
    assert!(q.workspace_bytes < 128 << 10);
    for axis in 0..5 {
        let mut l = limits();
        match axis {
            0 => l.inventory.leaves = q.leaves - 1,
            1 => l.inventory.raw_bytes = q.raw_bytes - 1,
            2 => l.inventory.hash_bytes = q.hash_bytes - 1,
            3 => l.inventory.work = q.work - 1,
            _ => l.inventory.workspace_bytes = q.workspace_bytes - 1,
        }
        let e = MappedFlatReplayV9::prepare(&t.view, &h, h.id(), std::path::Path::new("/missing-before-R-quotas"), l).err().unwrap();
        assert!(e.contains("inventory R preflight quota"), "{axis}: {e}");
    }
    let mut wrong_r = h.clone();
    wrong_r.model_inventory_root = kaspa_consensus_core::Hash64::from_bytes([77; 64]);
    assert!(
        MappedFlatReplayV9::prepare(&t.view, &wrong_r, wrong_r.id(), &t.path, limits())
            .err()
            .unwrap()
            .contains("inventory R root differs")
    );
    let own = MappedFlatReplayV9::prepare(&t.view, &h, h.id(), &t.path, limits()).unwrap();
    assert_eq!(own.usage().model_inventory, q);
    assert_eq!(own.usage().authenticated_model_inventory_root, Some(h.model_inventory_root));
}

#[test]
fn public_binding_substitution_is_refused_before_any_model_file() {
    let t = trial(0);
    let h = binding(&t);
    for axis in 0..7 {
        let mut wrong = h.clone();
        match axis {
            0 => wrong.descriptor[0] ^= 1,
            1 => wrong.program_root[0] ^= 1,
            2 => wrong.program_bytes_len += 1,
            3 => wrong.kernel_param_root = kaspa_consensus_core::Hash64::from_bytes([42; 64]),
            4 => wrong.pc_instances += 1,
            5 => wrong.refuted = true,
            _ => (),
        }
        let id = if axis == 6 { kaspa_consensus_core::Hash64::from_bytes([56; 64]) } else { wrong.id() };
        let e = MappedFlatReplayV9::prepare(&t.view, &wrong, id, std::path::Path::new("/missing-before-binding-check"), limits())
            .err()
            .unwrap();
        assert!(e.contains("public model binding"), "{axis}: {e}");
    }
}

#[test]
fn model_inventory_quotation_prices_split_rows_and_odd_tree() {
    let mut b = ProgramBuilder::new(16_386, HISTORY_BOUND_V1_SMALL);
    let param = b.param("split", DType::I32, &[3, 2, 8193], false);
    let mut pre = b.block("pre", vec![]);
    let row = pre.slice(param, 0, 0, 1);
    let row = pre.reshape_fixed(row, &[16_386]);
    let pre = pre.finish(&[row]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[16_386])]);
    let x = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(x);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    let q = mapped_model_inventory_quote_v9(&p).unwrap();
    // Three axis-0 rows of a rank-3 tensor, each split into 32768,32768,8:
    // nine leaves and eight interior hashes under odd promotion.
    assert_eq!(q.leaves, 9);
    assert_eq!(q.raw_bytes, 3 * 65_544);
    assert_eq!(q.hash_bytes, q.raw_bytes + 9 * 21 + 8 * 128);
    assert!(q.work > q.hash_bytes && q.workspace_bytes < 128 << 10);
}

#[test]
fn actual_qwen_inventory_quotation_is_geometry_only_and_bounded() {
    let p = misaka_palw_tir::TirProgramV1::decode_canonical(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../docs/design/palw/tir/evidence/qwen25-real-pilot-program.tir"
    )))
    .unwrap();
    let q = mapped_model_inventory_quote_v9(&p).unwrap();
    assert_eq!(q.leaves, 1_445_797);
    assert_eq!(q.raw_bytes, 1_835_516_100);
    assert!(q.hash_bytes > q.raw_bytes && q.hash_bytes < 4 << 30);
    assert!(q.work > q.hash_bytes && q.work < 1 << 40);
    assert!(q.workspace_bytes < 512 << 10);
    eprintln!("ACTUAL_QWEN_R_METADATA_ONLY {q:?}");
}
