//! Source-only regression cases for the consolidated validation phase.
use misaka_palw_kernel::{
    flat_detect39::*,
    flat_job39::*,
    flat_material39::*,
    flat_point39::*,
    flat_public39::*,
    flat_quota11::owning39::derive_quota_v39,
    flat_source39::*,
    flat_trace39::*,
    hash::{Digest, object_id},
    trace::ParamCommitmentsV1,
};
use misaka_palw_sdk::native_trace_archive_v39::DiskTraceArchiveV39;
use misaka_palw_tir::{
    ConstDecl, DType, Prim, Ref, Schedule, Tensor, TirError, TirErrorKind, TirResult,
    interp_v39::{ParamDescriptorV39, ParamSourceV39},
    program_v2::OutputDecl,
    program_v39::*,
};
struct Empty;
impl ParamSourceV39 for Empty {
    fn descriptor(&self, _: u16, _: Option<u16>) -> Option<ParamDescriptorV39> {
        None
    }
    fn read_range(&self, _: u16, _: Option<u16>, _: u64, _: usize) -> TirResult<Vec<i128>> {
        Err(TirError::new(TirErrorKind::Missing, "fixture has no parameters"))
    }
}
impl MaterialPageReaderV39 for Empty {
    fn page(&self, _: Digest, _: u16) -> Result<MaterialPageV39, String> {
        Err("fixture has no material".into())
    }
}
fn source() -> CheckedSourceV39 {
    let out = TensorTypeV39::scalar(DType::I16);
    let p = TirProgramV39 {
        version: 39,
        profile: SourceProfileV39::full_source(),
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: 16,
        history_bound: 32,
        inputs: vec![],
        params: vec![],
        consts: vec![ConstDecl { dtype: DType::I16, shape: vec![], data: 5i16.to_le_bytes().to_vec() }],
        states: vec![],
        blocks: vec![
            BlockV39 {
                name: "pre".into(),
                carry_in: vec![],
                nodes: vec![NodeV39 {
                    prim: NodePrimV39::Frozen(Prim::Cast),
                    inputs: vec![Ref::Const(0)],
                    out: out.clone(),
                    commit: true,
                }],
                carry_out: vec![0],
            },
            BlockV39 {
                name: "post".into(),
                carry_in: vec![out.clone()],
                nodes: vec![NodeV39 { prim: NodePrimV39::Frozen(Prim::Cast), inputs: vec![Ref::CarryIn(0)], out, commit: true }],
                carry_out: vec![],
            },
        ],
        schedule: Schedule { pre: 0, layers: vec![], post: 1 },
        output: OutputDecl::Final { node: 0 },
    };
    FlatSourceWireV39 {
        version: 39,
        program_bytes: p.encode(),
        inventory_root: derive_inventory_root_v39(&p, [9; 64], &[]).unwrap(),
        real_parameters: ParamCommitmentsV1::default(),
        original_context_max: 32,
        original_task_metadata: [9; 64],
        original_inventory: vec![],
    }
    .check()
    .unwrap()
}
fn maximum(s: &CheckedSourceV39) -> CheckedJobMaximumV39 {
    let manifest = MaterialManifestV39 { version: 39, source: s.root(), master: [4; 64], bindings: vec![] };
    let request = JobMaximumV39 {
        version: 39,
        source: s.root(),
        requester: [1; 64],
        producer: [2; 64],
        nonce: [3; 64],
        stop: RequestStopV39::Fixed { positions: 2, tokens: vec![0, 0] },
        input_manifest: manifest.root(),
    };
    CheckedJobMaximumV39::check(s, &request, &manifest).unwrap()
}
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("rfc02-own39-{}-{n}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn independent_replay_locates_original_carry_fault_and_self_checks_filing() {
    let s = source();
    let m = maximum(&s);
    let inputs = ActualInputResolverV39::derive(&s, &m, &[], &Empty).unwrap();
    let scratch = Scratch::new();
    let mut dishonest = DiskTraceArchiveV39::create(&scratch.0.join("producer"), &s, 1 << 20).unwrap();
    dishonest.begin(s.root(), m.root(), inputs.root()).unwrap();
    let a = Tensor::new(DType::I16, vec![], vec![5]).unwrap();
    let b = Tensor::new(DType::I16, vec![], vec![7]).unwrap();
    let ca = misaka_palw_kernel::merkle3::tensor_commitment_v3(&a);
    let cb = misaka_palw_kernel::merkle3::tensor_commitment_v3(&b);
    let mut roots = Vec::new();
    for p in 0..2 {
        dishonest.node(p, 0, 0, &a, ca).unwrap();
        dishonest.node(p, 1, 0, &b, cb).unwrap();
        let root = misaka_palw_kernel::seg::position_root_of_v1(p, &[vec![ca], vec![cb]]);
        dishonest.position_finished(p, root, &b).unwrap();
        roots.push(root);
    }
    let actual = misaka_palw_kernel::flat_resources39::derive_flat_resources_v39(&s, 2).unwrap();
    let claim = FlatClaimV39 {
        version: 39,
        source: s.root(),
        maximum: m.root(),
        generated: object_id(b"misaka-palw/kernel/actual-generated/v39", &inputs.generated()),
        maximum_scope: m.resources().scope().root(),
        actual_scope: actual.scope().root(),
        input_binding: inputs.root(),
        positions: 2,
        segment_roots: misaka_palw_kernel::seg::segment_roots_of_position_roots_v1(&roots),
    };
    dishonest.finish(&TraceSummaryV39 { claim: claim.clone(), position_roots: roots, output_roots: vec![cb; 2] }).unwrap();
    let mut own = DiskTraceArchiveV39::create(&scratch.0.join("auditor"), &s, 1 << 20).unwrap();
    let replay = execute_independent_replay_v39(&s, &m, &inputs, &Empty, &mut own).unwrap();
    let context = CheckedPointContextV39::check(&s, &m, claim.root(), &claim, &inputs).unwrap();
    let quota = derive_quota_v39(&s, &m, [5; 64], [6; 64], 32, 32).unwrap();
    let FindingV39::Fault { filing, conviction } = detect_first_fault_v39(&context, &quota, &replay, &mut dishonest).unwrap() else {
        panic!("wrong original carry value must produce a real proof")
    };
    assert_eq!(conviction, FlatConvictionV39::Computation);
    assert_eq!(check_flat_filing_v39(&context, &filing).unwrap(), conviction);
    let mut relabeled = claim.clone();
    relabeled.maximum = [8; 64];
    assert!(CheckedPointContextV39::check(&s, &m, relabeled.root(), &relabeled, &inputs).is_err());
}
#[test]
fn bounded_public_codecs_reject_vector_claims_before_large_allocation() {
    let mut demand = vec![4, 1]; // Branch, Nodes directory, NodeOpening prefix
    demand.extend_from_slice(&0u32.to_le_bytes());
    demand.extend_from_slice(&0u16.to_le_bytes());
    demand.extend_from_slice(&0u16.to_le_bytes());
    demand.extend_from_slice(&[1; 64]);
    demand.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatDemandV39::from_bytes(&demand).is_err());
    let mut reply = vec![2, 0, 2]; // Header, noNode, dtype
    reply.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatReplyV39::from_bytes(&reply).is_err());
    assert!(material_page_tree_root_v39(&[]).is_err());
    assert!(material_page_path_v39(&[[1; 64]], 1).is_err());
}
#[test]
fn upstream_derived_actual_does_not_release_original_requested_maximum() {
    let stop = RequestStopV39::Derived { maximum_positions: 32, tokens: vec![1, 2] };
    assert_eq!(stop.maximum_positions().unwrap(), 32);
    assert_eq!(stop.actual_tokens(&[], 16).unwrap(), vec![1, 2]);
    assert!(stop.actual_tokens(&[3], 16).is_err());
    assert!(RequestStopV39::Derived { maximum_positions: 1, tokens: vec![1, 2] }.maximum_positions().is_err());
}
