//! Independent primitive/state composition, bounded source conversion, and the existing engines.
mod common;
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, Prim, Ref, TensorType, TirProgramV1};
use misaka_palw_tir_lower::frontend_pack::{
    self, FrontendPack,
    program::{Operation, Program},
};
use misaka_palw_tir_lower::{
    admission, artifact,
    weights::{TensorMeta, TensorSource},
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn program() -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("embed", DType::I8, &[16, 4], false);
    let w = pb.param("weight", DType::I8, &[4, 4], true);
    let m = pb.param("multiplier", DType::I64, &[4], true);
    let head = pb.param("head", DType::I16, &[16, 4], false);
    let state = pb.fixed_state("accumulator", DType::I32, &[4], -4000, 4000, true);
    let hist = pb.hist_state("rows", DType::I32, &[4], 16, true);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(table, Ref::Input(0), 0, 0);
        let row = b.cast(row, DType::I32);
        b.finish(&[row])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let y = b.matmul(w, x, DType::I64);
        let y = b.reshape_fixed(y, &[4]);
        let y = b.mul(y, m, DType::I128);
        let y = b.shr(y, 20, misaka_palw_tir::Rounding::HalfAwayFromZero, DType::I128);
        let y = b.clamp(y, -200, 200, DType::I32);
        let y = b.add(y, Ref::State(state), DType::I32);
        let y = b.state_write(state, y);
        let rows = b.hist_append(hist, y);
        let y = b.reduce_max(rows, 0);
        let y = b.reshape_fixed(y, &[4]);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let y = b.matmul(head, x, DType::I64);
        let y = b.reshape_fixed(y, &[16]);
        let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(y);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    pb.finish(pre, vec![layer, layer], post, logits)
}

fn definition(p: &TirProgramV1) -> Value {
    let mut program = serde_json::to_value(Program::of(p)).unwrap();
    program["schedule"]["layers"] = json!({"$repeat":[p.schedule.layers[0],{"$cfg":"layers"}]});
    json!({"format":frontend_pack::FORMAT,"id":"unseen-composer","scope":{"task":"text-generation","completeness":"full","components":["decoder"]},
    "inert":["model_type"],"program":program,"bindings":{"$concat":[
        [{"param":0,"layer":null,"source":"stranger.table","import":{"kind":"integer"}},
         {"param":3,"layer":null,"source":"stranger.head","import":{"kind":"integer"}}],
        {"$flatten":{"$map":[{"$range":{"$cfg":"layers"}},"i",[
            {"param":1,"layer":{"$var":"i"},"source":{"$cat":["stranger.w.",{"$var":"i"}]},"import":{"kind":"integer"}},
            {"param":2,"layer":{"$var":"i"},"source":{"$cat":["stranger.m.",{"$var":"i"}]},"import":{"kind":"integer"}}
        ]]}}
    ]}})
}
fn config() -> Value {
    json!({"layers":2,"model_type":"UnpublishedFutureModel"})
}

struct Source {
    tensors: BTreeMap<String, (TensorMeta, Vec<u8>)>,
    reads: Cell<usize>,
    peak: Cell<usize>,
    limit: usize,
}
impl TensorSource for Source {
    fn names(&self) -> Vec<String> {
        self.tensors.keys().cloned().collect()
    }
    fn shape(&self, n: &str) -> Option<Vec<usize>> {
        self.tensors.get(n).map(|t| t.0.shape.clone())
    }
    fn metadata(&self, n: &str) -> Option<TensorMeta> {
        self.tensors.get(n).map(|t| t.0.clone())
    }
    fn load(&self, _: &str) -> misaka_palw_tir_lower::Result<misaka_palw_tir_lower::weights::Tensor> {
        panic!("whole/f32 loading must not occur")
    }
    fn read_slice(&self, n: &str, r: Range<u64>) -> misaka_palw_tir_lower::Result<Vec<u8>> {
        assert!(r.end - r.start <= self.limit as u64, "unbounded range");
        self.peak.set(self.peak.get().max((r.end - r.start) as usize));
        self.reads.set(self.reads.get() + 1);
        Ok(self.tensors[n].1[r.start as usize..r.end as usize].to_vec())
    }
}
fn source(p: &TirProgramV1) -> Source {
    let mut tensors = BTreeMap::new();
    for (j, instances) in misaka_palw_tir_artifact::param_instances_v1(p).into_iter().enumerate() {
        let param = &p.params[j];
        for layer in instances {
            let name = match j {
                0 => "stranger.table".into(),
                1 => format!("stranger.w.{}", layer.unwrap()),
                2 => format!("stranger.m.{}", layer.unwrap()),
                3 => "stranger.head".into(),
                _ => unreachable!(),
            };
            let n: usize = param.shape.iter().map(|n| *n as usize).product();
            let mut bytes = Vec::new();
            for i in 0..n {
                param.dtype.encode_le(
                    if j == 2 { 9_007_199_254_740_993 + i as i128 } else { ((i * 7 + j * 11) % 17) as i128 - 8 },
                    &mut bytes,
                );
            }
            tensors.insert(
                name,
                (
                    TensorMeta {
                        dtype: param.dtype.name().to_ascii_uppercase(),
                        shape: param.shape.iter().map(|n| *n as usize).collect(),
                        bytes: bytes.len() as u64,
                    },
                    bytes,
                ),
            );
        }
    }
    Source { tensors, reads: Cell::new(0), peak: Cell::new(0), limit: 1 << 20 }
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p =
            std::env::temp_dir().join(format!("palw-frontend-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn err(d: &Value, c: &Value, s: &Source) -> String {
    match FrontendPack::parse(&d.to_string()).and_then(|p| p.compile(c, s, &admission::default_inputs())) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("must refuse"),
    }
}

#[test]
fn independent_template_is_the_same_bytes_and_runs_on_three_engines_and_court() {
    let p = program();
    let s = source(&p);
    let d = definition(&p);
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let compiled = pack.compile(&config(), &s, &admission::default_inputs()).unwrap();
    assert_eq!(compiled.program().encode(), p.encode());
    assert_eq!(s.reads.get(), 0, "header-only preflight");
    let dir = Temp::new();
    let path = dir.0.join("model.palwtir");
    compiled.write(&path, &s, [17; 64], 8).unwrap();
    let (_, params) = artifact::read(&path, &p).unwrap();
    assert_eq!(params.tensors[&(2, Some(0))].le_bytes(), s.tensors["stranger.m.0"].1, "I64 bits above f64/f32 exactness retained");
    let seq: Vec<usize> = (0..32).map(|i| i % 16).collect();
    assert_eq!(common::three_ways(&p, &params, &[seq.clone()]).unwrap(), 32);
    let tokens: Vec<u32> = seq.into_iter().map(|n| n as u32).collect();
    common::court_coverage(&p, &params, &tokens, &[0, 1, 15, 16, 31], &[4, 16]).unwrap();
}

#[test]
fn block_sizes_have_identical_artifacts_and_receipts_and_mismatch_preserves_output() {
    let p = program();
    let mut s = source(&p);
    s.limit = 31;
    let compiled =
        FrontendPack::parse(&definition(&p).to_string()).unwrap().compile(&config(), &s, &admission::default_inputs()).unwrap();
    let dir = Temp::new();
    let a = dir.0.join("a");
    let b = dir.0.join("b");
    let x = compiled.write(&a, &s, [17; 64], 8).unwrap();
    let y = compiled.write_checked(&b, &s, [17; 64], 31, Some(&x.record)).unwrap();
    assert_eq!(x.record, y.record);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    assert!(s.peak.get() <= 31);
    s.tensors.get_mut("stranger.table").unwrap().1[0] ^= 1;
    let original = std::fs::read(&b).unwrap();
    let e = compiled.write_checked(&b, &s, [17; 64], 31, Some(&x.record)).err().unwrap().to_string();
    assert!(e.contains("FRONTEND_BUILD_MISMATCH"), "{e}");
    assert_eq!(std::fs::read(&b).unwrap(), original);
    assert!(std::fs::read_dir(&dir.0).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
}

#[test]
fn content_hash_ignores_whitespace_and_key_order_but_not_semantics() {
    let d = definition(&program());
    let a = FrontendPack::parse(&d.to_string()).unwrap();
    let b = FrontendPack::parse(&serde_json::to_string_pretty(&d).unwrap()).unwrap();
    assert_eq!(a.hash(), b.hash());
    let mut d = d;
    d["id"] = json!("another-author-label");
    assert_ne!(a.hash(), FrontendPack::parse(&d.to_string()).unwrap().hash());
}

#[test]
fn unread_config_and_tensors_missing_duplicate_or_wrong_bindings_refuse_before_data() {
    let p = program();
    let mut s = source(&p);
    let d = definition(&p);
    let mut c = config();
    c["future_math"] = json!(1);
    assert!(err(&d, &c, &s).contains("future_math"));
    let mut strict = d.clone();
    strict["inert"] = json!([]);
    assert!(err(&strict, &config(), &s).contains("model_type"));
    let mut wrong = d.clone();
    wrong["bindings"] = json!([]);
    assert!(err(&wrong, &config(), &s).contains("missing parameter"));
    let mut wrong = d.clone();
    wrong["bindings"] = {
        let mut a = d["bindings"]["$concat"][0].as_array().unwrap().clone();
        a.push(a[0].clone());
        json!(a)
    };
    assert!(err(&wrong, &config(), &s).contains("repeated parameter"));
    s.tensors.get_mut("stranger.table").unwrap().0.shape = vec![u32::MAX as usize; 4];
    assert!(err(&d, &config(), &s).contains("shape"));
    let mut s = source(&p);
    s.tensors.get_mut("stranger.table").unwrap().0.bytes = u64::MAX;
    assert!(err(&d, &config(), &s).contains("byte count"));
    let mut s = source(&p);
    s.tensors.insert("secret.unread".into(), (TensorMeta { dtype: "I8".into(), shape: vec![1], bytes: 1 }, vec![0]));
    assert!(err(&d, &config(), &s).contains("TENSOR_UNREAD"));
    let mut s = source(&p);
    s.tensors.remove("stranger.head");
    assert!(err(&d, &config(), &s).contains("missing tensor"));
    assert_eq!(s.reads.get(), 0);
}

#[test]
fn unknown_grammar_and_oversized_graph_use_explicit_refusals() {
    let p = program();
    let s = source(&p);
    let d = definition(&p);
    let mut wrong = d.clone();
    wrong["exec_code"] = json!("arbitrary script");
    assert!(err(&wrong, &config(), &s).contains("unknown field"));
    let mut wrong = d.clone();
    wrong["program"]["blocks"][0]["nodes"][0]["prim"] = json!({"op":"FutureOp"});
    assert!(err(&wrong, &config(), &s).contains("KERNEL_EXTENSION_REQUIRED"));
    let mut wrong = d.clone();
    wrong["program"]["prim_set_id"] = json!("01".repeat(64));
    assert!(err(&wrong, &config(), &s).contains("KERNEL_EXTENSION_REQUIRED"));
    let mut wrong = d.clone();
    wrong["program"]["blocks"][0]["nodes"][0]["out"]["shape"] = json!(["patches"]);
    assert!(err(&wrong, &config(), &s).contains("KERNEL_EXTENSION_REQUIRED"));
    let mut wrong = d.clone();
    wrong["program"]["blocks"] = json!({"$repeat":[{"$range":1024},1_048_576]});
    assert!(err(&wrong, &config(), &s).contains("FRONTEND_EXPANSION_LIMIT"));
    let mut wrong = d.clone();
    wrong["program"]["params"][0]["shape"] = json!([u32::MAX, u32::MAX, u32::MAX, u32::MAX]);
    assert!(err(&wrong, &config(), &s).contains("TIR_ADMISSION"));
    assert_eq!(s.reads.get(), 0);
}

#[test]
fn the_callers_resource_profile_is_enforced_before_weight_reads() {
    let p = program();
    let s = source(&p);
    let pack = FrontendPack::parse(&definition(&p).to_string()).unwrap();
    let mut inputs = admission::default_inputs();
    inputs.ceilings.max_position_macs = 1;
    let e = pack.compile(&config(), &s, &inputs).err().unwrap().to_string();
    assert!(e.contains("TIR_ADMISSION"), "{e}");
    assert_eq!(s.reads.get(), 0);
}

#[test]
fn nested_scopes_do_not_import_hf_inert_assumptions() {
    let p = program();
    let s = source(&p);
    let mut d = definition(&p);
    let mut c = config();
    c["nested"] = json!({"layers":2,"model_type":"may-change-math"});
    d["vars"] = json!({"nested_layers":{"$scope":{"key":"nested","body":{"$cfg":"layers"}}}});
    assert!(err(&d, &c, &s).contains("nested.model_type"));
    d["vars"]["nested_layers"]["$scope"]["inert"] = json!(["model_type"]);
    FrontendPack::parse(&d.to_string()).unwrap().compile(&c, &s, &admission::default_inputs()).unwrap();
    assert_eq!(s.reads.get(), 0);
}

#[test]
fn all_frozen_primitive_attributes_roundtrip_and_unknown_attributes_refuse() {
    use misaka_palw_tir::prim::{Cmp, Rounding};
    let ops = vec![
        Prim::Reshape,
        Prim::Transpose { perm: vec![1, 0] },
        Prim::Slice { axis: 2, start: 3 },
        Prim::Concat { axis: 1 },
        Prim::Broadcast,
        Prim::Iota { axis: 0, start: -5, step: 7 },
        Prim::Gather { axis: 2, batch_dims: 1 },
        Prim::Cast,
        Prim::Add,
        Prim::Sub,
        Prim::Mul,
        Prim::MatMul,
        Prim::ReduceSum { axis: 1 },
        Prim::ReduceMax { axis: 1 },
        Prim::Div { rule: Rounding::HalfAwayFromZero },
        Prim::Clamp { lo: -100, hi: 200 },
        Prim::Log2Floor,
        Prim::IntExp,
        Prim::IntRsqrt,
        Prim::IntLn,
        Prim::Compare { cmp: Cmp::Ge },
        Prim::Select,
        Prim::TopK { axis: 1, k: 3 },
        Prim::StateWrite { state: 4 },
        Prim::HistAppend { state: 5 },
    ];
    assert_eq!(ops.len(), 25);
    for p in ops {
        let v = serde_json::to_value(Operation::of(&p)).unwrap();
        let decoded: Operation = serde_json::from_value(v).unwrap();
        assert_eq!(decoded.compile(), p);
    }
    assert!(serde_json::from_value::<Operation>(json!({"op":"Add","backend_code":"x"})).is_err());
    assert!(serde_json::from_value::<Operation>(json!({"op":"TopK","axis":0,"k":2,"tie":"random"})).is_err());
}

#[test]
fn explicit_ieee_import_records_saturation_and_rejects_nan_or_implicit_narrowing() {
    let p = program();
    let mut s = source(&p);
    let mut d = definition(&p);
    let binding = &mut d["bindings"]["$concat"][0][0];
    binding["import"] = json!({"kind":"fixed_point","shift":0,"round":"half_away_from_zero","overflow":"saturate"});
    let t = s.tensors.get_mut("stranger.table").unwrap();
    t.0.dtype = "F64".into();
    t.1 = (0..64).flat_map(|i| if i % 2 == 0 { 2.5f64.to_le_bytes() } else { 1e300f64.to_le_bytes() }).collect();
    t.0.bytes = t.1.len() as u64;
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let c = pack.compile(&config(), &s, &admission::default_inputs()).unwrap();
    let dir = Temp::new();
    let path = dir.0.join("model");
    let out = c.write(&path, &s, [0; 64], 24).unwrap();
    assert_eq!(out.record.saturated_values, 32);
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open(&path).unwrap();
    assert_eq!(&container.read_tensor_bytes(0, None).unwrap()[..4], &[3, 127, 3, 127]);
    s.tensors.get_mut("stranger.table").unwrap().1[..8].copy_from_slice(&f64::NAN.to_le_bytes());
    let e = c.write(&path, &s, [0; 64], 24).err().unwrap().to_string();
    assert!(e.contains("FRONTEND_QUANT_NONFINITE"), "{e}");
    d["bindings"]["$concat"][0][0]["import"]["overflow"] = json!("reject");
    s.tensors.get_mut("stranger.table").unwrap().1[..8].copy_from_slice(&1e300f64.to_le_bytes());
    let c = FrontendPack::parse(&d.to_string()).unwrap().compile(&config(), &s, &admission::default_inputs()).unwrap();
    assert!(c.write(&path, &s, [0; 64], 24).err().unwrap().to_string().contains("FRONTEND_QUANT_RANGE"));
}

#[test]
fn cli_reproduces_a_real_safetensors_file_without_an_adapter_match() {
    let dir = Temp::new();
    let p = program();
    let s = source(&p);
    let mut header = serde_json::Map::new();
    let mut data = Vec::new();
    for (name, (meta, bytes)) in &s.tensors {
        let start = data.len();
        data.extend_from_slice(bytes);
        header.insert(name.clone(), json!({"dtype":meta.dtype,"shape":meta.shape,"data_offsets":[start,data.len()]}));
    }
    let header = serde_json::to_vec(&header).unwrap();
    let mut file = (header.len() as u64).to_le_bytes().to_vec();
    file.extend_from_slice(&header);
    file.extend_from_slice(&data);
    std::fs::write(dir.0.join("model.safetensors"), file).unwrap();
    std::fs::write(dir.0.join("config.json"), config().to_string()).unwrap();
    let pack = dir.0.join("frontend.json");
    std::fs::write(&pack, definition(&p).to_string()).unwrap();
    let out = dir.0.join("model.palwtir");
    let receipt = dir.0.join("receipt.json");
    let run = |output: &std::path::Path, record: &std::path::Path, block: &str, expected: Option<&std::path::Path>| {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_palw-tir-frontend"));
        cmd.arg("--frontend-pack")
            .arg(&pack)
            .arg(&dir.0)
            .arg("--out")
            .arg(output)
            .arg("--record")
            .arg(record)
            .arg("--block-bytes")
            .arg(block);
        if let Some(e) = expected {
            cmd.arg("--expect-record").arg(e);
        }
        let r = cmd.output().unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        let report: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(report["source_equivalence"], "SOURCE_EQUIVALENCE_UNVERIFIED");
    };
    run(&out, &receipt, "8", None);
    let other = dir.0.join("peer.palwtir");
    run(&other, &dir.0.join("peer.json"), "127", Some(&receipt));
    assert_eq!(std::fs::read(out).unwrap(), std::fs::read(other).unwrap());
    let before = std::fs::read(dir.0.join("model.safetensors")).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_palw-tir-frontend"))
        .arg("--frontend-pack")
        .arg(&pack)
        .arg(&dir.0)
        .arg("--out")
        .arg(dir.0.join("model.safetensors"))
        .arg("--record")
        .arg(dir.0.join("failed.json"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("FRONTEND_OUTPUT_CONFLICT"));
    assert_eq!(std::fs::read(dir.0.join("model.safetensors")).unwrap(), before);
}
