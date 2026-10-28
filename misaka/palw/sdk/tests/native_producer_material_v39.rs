//! Source-only regression cases; execution belongs to the consolidated phase.
#![allow(warnings)]
use misaka_palw_kernel::{
    flat_job39::*, flat_material_output39::OutputProjectionV39, flat_material39::*, flat_source39::*, hash::Digest,
    trace::ParamCommitmentsV1,
};
use misaka_palw_sdk::{native_producer_v39::produce_reference_v39, native_trace_archive_v39::DiskTraceArchiveV39};
use misaka_palw_tir::{
    ConstDecl, DType, Prim, Ref, Schedule, TirError, TirErrorKind, TirResult,
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
        Err(TirError::new(TirErrorKind::Missing, "empty original source"))
    }
}
impl MaterialPageReaderV39 for Empty {
    fn page(&self, _: Digest, _: u16) -> Result<MaterialPageV39, String> {
        Err("empty original inputs".into())
    }
}
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("rfc02-producer39-{}-{t}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn source(logits: bool) -> CheckedSourceV39 {
    let ty = TensorTypeV39::fixed(DType::I32, &[4]);
    let p = TirProgramV39 {
        version: 39,
        profile: SourceProfileV39::full_source(),
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: 4,
        history_bound: 8,
        inputs: vec![],
        params: vec![],
        states: vec![],
        consts: vec![ConstDecl {
            dtype: DType::I32,
            shape: vec![4],
            data: [1i32, 9, 9, 0].iter().flat_map(|n| n.to_le_bytes()).collect(),
        }],
        blocks: vec![
            BlockV39 {
                name: "pre".into(),
                carry_in: vec![],
                nodes: vec![NodeV39 {
                    prim: NodePrimV39::Frozen(Prim::Cast),
                    inputs: vec![Ref::Const(0)],
                    out: ty.clone(),
                    commit: true,
                }],
                carry_out: vec![0],
            },
            BlockV39 {
                name: "post".into(),
                carry_in: vec![ty.clone()],
                nodes: vec![NodeV39 { prim: NodePrimV39::Frozen(Prim::Cast), inputs: vec![Ref::CarryIn(0)], out: ty, commit: true }],
                carry_out: vec![],
            },
        ],
        schedule: Schedule { pre: 0, layers: vec![], post: 1 },
        output: if logits { OutputDecl::Logits { node: 0, scheme_id: [1; 64] } } else { OutputDecl::Rows { node: 0 } },
    };
    FlatSourceWireV39 {
        version: 39,
        program_bytes: p.encode(),
        inventory_root: derive_inventory_root_v39(&p, [9; 64], &[]).unwrap(),
        real_parameters: ParamCommitmentsV1::default(),
        original_context_max: 8,
        original_task_metadata: [9; 64],
        original_inventory: vec![],
    }
    .check()
    .unwrap()
}
fn maximum(s: &CheckedSourceV39, stop: RequestStopV39) -> CheckedJobMaximumV39 {
    let m = MaterialManifestV39 { version: 39, source: s.root(), master: [4; 64], bindings: vec![] };
    CheckedJobMaximumV39::check(
        s,
        &JobMaximumV39 {
            version: 39,
            source: s.root(),
            requester: [1; 64],
            producer: [2; 64],
            nonce: [3; 64],
            stop,
            input_manifest: m.root(),
        },
        &m,
    )
    .unwrap()
}
#[test]
fn real_greedy_tie_uses_lowest_id_and_stops_at_first_original_eos() {
    let s = source(true);
    let m = maximum(&s, RequestStopV39::Decode { prompt: vec![0, 0], requested_new_tokens: 4, eos: vec![1] });
    let scratch = Scratch::new();
    let mut archive = DiskTraceArchiveV39::create(&scratch.0.join("trace"), &s, 8 << 20).unwrap();
    let p = produce_reference_v39(&s, &m, &Empty, &Empty, &mut archive).unwrap();
    assert_eq!(p.generated, vec![1]);
    assert_eq!(p.trace.claim.positions, 2);
    assert_eq!(m.maximum_positions(), 5);
    let actual = ActualInputResolverV39::derive(&s, &m, &p.generated, &Empty).unwrap();
    let reopened = DiskTraceArchiveV39::open_existing(&scratch.0.join("trace"), &s, &m, &actual, &p.trace.claim, 8 << 20).unwrap();
    assert!(reopened.node_opening(1, 1, 0).is_ok());
    assert!(reopened.node_opening(2, 1, 0).is_err());
}
#[test]
fn requested_maximum_stops_without_fabricating_an_eos() {
    let s = source(true);
    let m = maximum(&s, RequestStopV39::Decode { prompt: vec![0, 0], requested_new_tokens: 4, eos: vec![3] });
    let scratch = Scratch::new();
    let mut archive = DiskTraceArchiveV39::create(&scratch.0.join("trace"), &s, 8 << 20).unwrap();
    let p = produce_reference_v39(&s, &m, &Empty, &Empty, &mut archive).unwrap();
    assert_eq!(p.generated, vec![1, 1, 1, 1]);
    assert_eq!(p.trace.claim.positions, 5);
}
#[test]
fn actual_rows_material_carries_all_original_openings_and_zero_padding() {
    use misaka_palw_sdk::{native_material_v39::MaterialSeedV39, native_upstream_material_v39::prepare_upstream_material_v39};
    let s = source(false);
    let m = maximum(&s, RequestStopV39::Fixed { positions: 2, tokens: vec![0, 0] });
    let scratch = Scratch::new();
    let mut archive = DiskTraceArchiveV39::create(&scratch.0.join("trace"), &s, 8 << 20).unwrap();
    let p = produce_reference_v39(&s, &m, &Empty, &Empty, &mut archive).unwrap();
    let seed = MaterialSeedV39 {
        version: 39,
        source: [7; 64],
        master: [4; 64],
        requester: [1; 64],
        requester_operator: [2; 64],
        nonce: [3; 64],
        input: 0,
        dtype: DType::I32,
        shape: vec![4, 4],
        origin: MaterialOriginV39::Upstream {
            master: [4; 64],
            stage: 0,
            job: [5; 64],
            class: [6; 64],
            source: s.root(),
            claim: p.trace.claim.root(),
            final_book: [8; 64],
            single_credit: [9; 64],
            output: [0; 64],
        },
    };
    let report = prepare_upstream_material_v39(
        &s,
        &p.trace.claim,
        &archive,
        OutputProjectionV39::Rows { rows_from: 0, drop: 0, pad_to: Some(4) },
        &seed,
        1,
        100,
        1 << 20,
        &scratch.0.join("material"),
    )
    .unwrap();
    assert_eq!(report["a6_eligible"], false);
    let raw = std::fs::read(scratch.0.join("material/original-output39.raw")).unwrap();
    let values = raw.chunks_exact(4).map(|w| i32::from_le_bytes(w.try_into().unwrap())).collect::<Vec<_>>();
    assert_eq!(&values[..8], &[1, 9, 9, 0, 1, 9, 9, 0]);
    assert_eq!(&values[8..], &[0; 8]);
    assert!(scratch.0.join("material/prepared/certificate-page-0000.borsh").exists());
}
