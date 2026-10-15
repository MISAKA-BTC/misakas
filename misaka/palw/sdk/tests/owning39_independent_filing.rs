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
    dishonest.finish(&TraceSummaryV39 { claim: claim.clone(), position_roots: roots, output_roots: vec![cb] }).unwrap();
    let mut own = DiskTraceArchiveV39::create(&scratch.0.join("auditor"), &s, 1 << 20).unwrap();
    let replay = execute_independent_replay_v39(&s, &m, &inputs, &Empty, &mut own).unwrap();
    let context = CheckedPointContextV39::check(&s, &m, claim.root(), &claim, &inputs).unwrap();
    let quota = derive_quota_v39(&s, &m, [5; 64], [6; 64], 32, 32).unwrap();
    let mut dishonest = DiskTraceArchiveV39::open_existing(&scratch.0.join("producer"), &s, &m, &inputs, &claim, 1 << 20).unwrap();
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

#[test]
fn disk_tensor_preserves_odd_merkle_promotion_and_rejects_changed_raw_leaf() {
    use misaka_palw_sdk::native_tensor_store_v39::DiskTensorV39;
    let scratch = Scratch::new();
    let tensor = Tensor::new(DType::I16, vec![16385], (0..16385).map(|n| (n % 101) as i128).collect()).unwrap();
    let root = misaka_palw_kernel::merkle3::tensor_commitment_v3(&tensor);
    let store =
        DiskTensorV39::create(&scratch.0.join("tensor"), tensor.dtype, &tensor.shape, root, 1 << 20, 1 << 20, |offset, count| {
            Ok(tensor.data[offset as usize..offset as usize + count].to_vec())
        })
        .unwrap();
    assert_eq!(store.subtree(0, 0, 5).unwrap(), store.header().row_root);
    for tile in 0..5 {
        assert!(store.leaf(0, 0, tile).unwrap().authenticates(&root));
    }
    let reopened = DiskTensorV39::open_existing(&scratch.0.join("tensor"), root, 1 << 20, 1 << 20).unwrap();
    assert!(reopened.leaf(1, 0, 4).unwrap().authenticates(&root));
    use std::io::{Seek, Write};
    let mut file = std::fs::OpenOptions::new().write(true).open(scratch.0.join("tensor/raw39")).unwrap();
    file.seek(std::io::SeekFrom::Start(16384 * 2)).unwrap();
    file.write_all(&(-7i16).to_le_bytes()).unwrap();
    drop(file);
    assert!(reopened.leaf(0, 0, 4).is_err());
}

#[test]
fn upstream_certificate_authenticates_projected_rows_and_zero_padding() {
    use misaka_palw_kernel::flat_material_output39::*;
    let base = source();
    let mut wire = base.wire().clone();
    let mut program = TirProgramV39::decode_canonical(&wire.program_bytes).unwrap();
    program.output = OutputDecl::Rows { node: 0 };
    wire.program_bytes = program.encode();
    let s = wire.check().unwrap();
    let m = maximum(&s);
    let inputs = ActualInputResolverV39::derive(&s, &m, &[], &Empty).unwrap();
    let scratch = Scratch::new();
    let mut archive = DiskTraceArchiveV39::create(&scratch.0.join("upstream"), &s, 1 << 20).unwrap();
    archive.begin(s.root(), m.root(), inputs.root()).unwrap();
    let summary = trace_streamed_v39(&s, &m, &inputs, &Empty, &mut archive).unwrap();
    archive.finish(&summary).unwrap();
    let opening = archive.node_opening(1, 1, 0).unwrap();
    let body = borsh::to_vec(&opening).unwrap();
    let mut certificate = 39u16.to_le_bytes().to_vec();
    certificate.extend_from_slice(&1u32.to_le_bytes());
    certificate.extend_from_slice(&(body.len() as u32).to_le_bytes());
    certificate.extend(body);
    struct Pages {
        material: Digest,
        certificate: Digest,
        raw: Vec<u8>,
        proof: Vec<u8>,
    }
    impl MaterialPageReaderV39 for Pages {
        fn page(&self, id: Digest, index: u16) -> Result<MaterialPageV39, String> {
            if id != self.material || index != 0 {
                return Err("wrong original material page".into());
            }
            Ok(MaterialPageV39 { index, body: self.raw.clone(), siblings: vec![] })
        }
    }
    impl OutputCertificatePageReaderV39 for Pages {
        fn page(&self, id: Digest, index: u16) -> Result<MaterialPageV39, String> {
            if id != self.certificate || index != 0 {
                return Err("wrong original output certificate page".into());
            }
            Ok(MaterialPageV39 { index, body: self.proof.clone(), siblings: vec![] })
        }
    }
    let run = |padding: i128| {
        let tensor = Tensor::new(DType::I16, vec![2], vec![5, padding]).unwrap();
        let tensor_root = misaka_palw_kernel::merkle3::tensor_commitment_v3(&tensor);
        let mut plan = OutputCertificatePlanV39 {
            version: 39,
            source: s.root(),
            claim: summary.claim.root(),
            output_declaration: object_id(b"misaka-palw/kernel/original-output-declaration/v39", &(s.root(), &s.program().output)),
            tensor: tensor_root,
            dtype: DType::I16,
            shape: vec![2],
            projection: OutputProjectionV39::Rows { rows_from: 0, drop: 1, pad_to: Some(2) },
            count: 1,
            bytes: certificate.len() as u64,
            page_root: [0; 64],
        };
        plan.page_root = output_certificate_page_leaf_v39(&plan, 0, &certificate);
        let mut descriptor = MaterialDescriptorV39 {
            version: 39,
            source: [24; 64],
            master: [20; 64],
            requester: [21; 64],
            requester_operator: [22; 64],
            nonce: [23; 64],
            input: 0,
            dtype: DType::I16,
            shape: vec![2],
            tensor: tensor_root,
            page_root: [0; 64],
            origin: MaterialOriginV39::Upstream {
                master: [20; 64],
                stage: 0,
                job: [25; 64],
                class: [26; 64],
                source: s.root(),
                claim: summary.claim.root(),
                final_book: [27; 64],
                single_credit: [28; 64],
                output: plan.root(),
            },
        };
        let raw = [5i16, padding as i16].into_iter().flat_map(i16::to_le_bytes).collect::<Vec<_>>();
        descriptor.page_root = material_page_leaf_v39(&descriptor, 0, &raw);
        let checked = CheckedOutputCertificatePlanV39::check(&s, &summary.claim, &descriptor, &plan).unwrap();
        let pages = Pages { material: descriptor.root(), certificate: plan.root(), raw, proof: certificate.clone() };
        check_upstream_material_output_v39(&s, summary.claim.root(), &summary.claim, &descriptor, &checked, &pages, &pages)
    };
    assert!(run(0).is_ok());
    assert!(run(3).is_err());
    // Merkle identity of authentic data authorizes no Final or validator vote.
    let mut relabeled = summary.claim.clone();
    relabeled.maximum = [30; 64];
    assert!(CheckedPointContextV39::check(&s, &m, relabeled.root(), &relabeled, &inputs).is_err());
}
