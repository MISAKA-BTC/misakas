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

fn nibble_descriptor() -> Value {
    let values: Vec<u8> = [-8f32, 7.0, 0.0, -1.0].into_iter().flat_map(f32::to_le_bytes).collect();
    json!({"schema":"misaka.palw.quant-format.v1","name":"OutsiderNibble","layout":{
        "kind":"virtual","roles":[{"name":"codes","suffix":".not-a-model-rule","dtypes":["U8"],"rank":2}],
        "axes":["o","i"],"shape":["dim_codes[0]","dim_codes[1]*2"]},
        "params":{"bias":{"config":"bias","default":8,"allowed":[8]}},
        "decode":{"target":"tensor","value":"((codes[o, i/2] >> (4*(i%2))) & 15) - bias"},
        "tests":[{"roles":{"codes":{"dtype":"U8","shape":[1,2],"hex":"f078"}},"config":{"bias":8},
            "values_f32_hex":frontend_pack::program::hex(&values)}]})
}
fn packed_case() -> (TirProgramV1, Value, Source, Source) {
    let p = program();
    let mut plain = source(&p);
    for l in 0..2 {
        plain.tensors.get_mut(&format!("stranger.w.{l}")).unwrap().1 =
            (0..16).map(|i| ((i * 3 + l * 5) % 16) as i8 - 8).map(|v| v as u8).collect();
    }
    let mut packed = source(&p);
    for l in 0..2 {
        packed.tensors.remove(&format!("stranger.w.{l}"));
        let raw: Vec<u8> = plain.tensors[&format!("stranger.w.{l}")]
            .1
            .chunks_exact(2)
            .map(|b| ((b[0] as i8 + 8) as u8) | (((b[1] as i8 + 8) as u8) << 4))
            .collect();
        packed.tensors.insert(format!("packed.{l}"), (TensorMeta { dtype: "U8".into(), shape: vec![4, 2], bytes: 8 }, raw));
    }
    let mut d = definition(&p);
    d["quant_formats"] = json!({"outsider":nibble_descriptor()});
    let imports = &mut d["bindings"]["$concat"][1]["$flatten"]["$map"][2][0];
    imports.as_object_mut().unwrap().remove("source");
    imports["import"] = json!({"kind":"descriptor","format":"outsider","roles":{"codes":{"$cat":["packed.",{"$var":"i"}]}},
        "config":{"bias":8},"shift":0,"round":"half_away_from_zero","overflow":"reject"});
    (p, d, packed, plain)
}

#[test]
fn an_unknown_packed_descriptor_streams_to_the_same_tir_and_three_engines() {
    let (p, d, mut packed, plain) = packed_case();
    packed.limit = 31;
    let compiled = FrontendPack::parse(&d.to_string()).unwrap().compile(&config(), &packed, &admission::default_inputs()).unwrap();
    assert_eq!(packed.reads.get(), 0);
    let dir = Temp::new();
    let a = dir.0.join("packed");
    let b = dir.0.join("peer");
    let c = dir.0.join("direct");
    let first = compiled.write(&a, &packed, [17; 64], 8).unwrap();
    let peer = compiled.write_checked(&b, &packed, [17; 64], 31, Some(&first.record)).unwrap();
    assert_eq!(first.record, peer.record);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    assert_eq!(first.record.quant_formats.len(), 1);
    assert_eq!(first.record.source_tensors.len(), 6);
    assert!(first.source_read_bytes > first.tensor_bytes, "pin scans and cache traffic are measured separately");
    assert!(packed.peak.get() <= 31);
    FrontendPack::parse(&definition(&p).to_string())
        .unwrap()
        .compile(&config(), &plain, &admission::default_inputs())
        .unwrap()
        .write(&c, &plain, [17; 64], 8)
        .unwrap();
    let (_, actual) = artifact::read(&a, &p).unwrap();
    let (_, expected) = artifact::read(&c, &p).unwrap();
    for (id, t) in &actual.tensors {
        assert_eq!(t.le_bytes(), expected.tensors[id].le_bytes());
    }
    let seq: Vec<usize> = (0..20).map(|i| i % 16).collect();
    common::three_ways(&p, &actual, &[seq.clone()]).unwrap();
    common::court_coverage(&p, &actual, &seq.into_iter().map(|n| n as u32).collect::<Vec<_>>(), &[0, 15, 16, 19], &[4, 16]).unwrap();
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["doc"] = json!("changed source spec");
    let changed = FrontendPack::parse(&wrong.to_string()).unwrap().compile(&config(), &packed, &admission::default_inputs()).unwrap();
    let previous = std::fs::read(&b).unwrap();
    assert!(
        changed
            .write_checked(&b, &packed, [17; 64], 8, Some(&first.record))
            .err()
            .unwrap()
            .to_string()
            .contains("FRONTEND_BUILD_MISMATCH")
    );
    assert_eq!(std::fs::read(&b).unwrap(), previous);
}

#[test]
fn mxfp4_expert_axes_use_the_pinned_independent_vector_without_whole_role_loads() {
    let descriptor: Value = serde_json::from_str(include_str!("../quant-formats/mxfp4_hf.json")).unwrap();
    let vector = &descriptor["tests"][0];
    let mut pb = ProgramBuilder::new(64, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("packed.experts", DType::I16, &[2, 64, 4], false);
    let w = pb.param("w", DType::I8, &[4, 4], true);
    let head = pb.param("head", DType::I16, &[64, 4], false);
    let expert = pb.scalar(DType::Idx, 1);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let t = b.gather(table, expert, 0, 0);
        let y = b.gather(t, Ref::Input(0), 0, 0);
        let y = b.cast(y, DType::I32);
        b.finish(&[y])
    };
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let y = b.matmul(w, x, DType::I64);
        let y = b.reshape_fixed(y, &[4]);
        let y = b.clamp(y, -10000, 10000, DType::I32);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let y = b.matmul(head, x, DType::I64);
        let y = b.reshape_fixed(y, &[64]);
        let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(y);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let p = pb.finish(pre, vec![layer], post, logits);
    let mut tensors = BTreeMap::new();
    for r in ["blocks", "scales"] {
        let t = &vector["roles"][r];
        let raw = frontend_pack::program::unhex(t["hex"].as_str().unwrap(), 64 << 10).unwrap();
        tensors.insert(
            r.to_string(),
            (
                TensorMeta { dtype: "U8".into(), shape: serde_json::from_value(t["shape"].clone()).unwrap(), bytes: raw.len() as u64 },
                raw,
            ),
        );
    }
    tensors.insert("w".into(), (TensorMeta { dtype: "I8".into(), shape: vec![4, 4], bytes: 16 }, vec![1; 16]));
    tensors.insert("head".into(), (TensorMeta { dtype: "I16".into(), shape: vec![64, 4], bytes: 512 }, vec![0; 512]));
    let s = Source { tensors, reads: Cell::new(0), peak: Cell::new(0), limit: 17 };
    let d = json!({"format":frontend_pack::FORMAT,"id":"expert-axis-import","scope":{"task":"text-generation","completeness":"partial","components":["packed-expert-read"]},
        "quant_formats":{"ocp":descriptor},"program":Program::of(&p),"bindings":[
        {"param":0,"layer":null,"import":{"kind":"descriptor","format":"ocp","roles":{"blocks":"blocks","scales":"scales"},"config":{},"shift":0,"round":"half_away_from_zero","overflow":"reject"}},
        {"param":1,"layer":0,"source":"w","import":{"kind":"integer"}},{"param":2,"layer":null,"source":"head","import":{"kind":"integer"}}]});
    let compiled = FrontendPack::parse(&d.to_string()).unwrap().compile(&json!({}), &s, &admission::default_inputs()).unwrap();
    assert_eq!(s.reads.get(), 0);
    let dir = Temp::new();
    let a = dir.0.join("mxfp4");
    let output = compiled.write(&a, &s, [0; 64], 16).unwrap();
    assert!(output.max_read_bytes <= 16);
    let (_, params) = artifact::read(&a, &p).unwrap();
    let want = frontend_pack::program::unhex(vector["values_f32_hex"].as_str().unwrap(), 64 << 10).unwrap();
    let expected: Vec<i128> = want.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap()).round() as i128).collect();
    assert_eq!(params.tensors[&(0, None)].le_bytes(), expected.into_iter().flat_map(|n| (n as i16).to_le_bytes()).collect::<Vec<_>>());
    common::three_ways(&p, &params, &[vec![0, 1, 31, 32, 63]]).unwrap();
    common::court_coverage(&p, &params, &[0, 1, 31, 32, 63], &[0, 4], &[4]).unwrap();
    let previous = std::fs::read(&a).unwrap();
    assert!(compiled.write(&a, &s, [0; 64], 8).err().unwrap().to_string().contains("one element per descriptor role"));
    assert_eq!(std::fs::read(&a).unwrap(), previous);
}

#[test]
fn descriptor_headers_vectors_config_and_expansion_refuse_before_checkpoint_reads() {
    let (_, d, mut s, _) = packed_case();
    let mut cases = Vec::new();
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["tests"] = json!([]);
    cases.push((wrong, "FRONTEND_DESCRIPTOR_LIMIT"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["tests"][0]["values_f32_hex"] = json!("00".repeat(16));
    cases.push((wrong, "fails"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["tests"][0]["roles"]["codes"]["shape"] = json!([usize::MAX, 2]);
    cases.push((wrong, "FRONTEND_DESCRIPTOR_LIMIT"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["decode"]["value"] = json!("1+".repeat(100) + "1");
    cases.push((wrong, "FRONTEND_DESCRIPTOR_LIMIT"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["decode"]["value"] = json!("(".repeat(40) + "1" + &")".repeat(40));
    cases.push((wrong, "FRONTEND_DESCRIPTOR_LIMIT"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["layout"]["shape"][0] = json!("codes[0,0]");
    cases.push((wrong, "data-dependent"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["layout"]["shape"][0] = json!("o+1");
    cases.push((wrong, "lane-dependent"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["layout"] = json!({"kind":"blocks","elems":32,"bytes":1,"fields":[]});
    cases.push((wrong, "EXTENSION_REQUIRED"));
    let mut wrong = d.clone();
    wrong["bindings"]["$concat"][1]["$flatten"]["$map"][2][0]["import"]["roles"]["hidden"] = json!("packed.0");
    cases.push((wrong, "undeclared descriptor role"));
    let mut wrong = d.clone();
    wrong["bindings"]["$concat"][1]["$flatten"]["$map"][2][0]["import"]["config"]["unread"] = json!(1);
    cases.push((wrong, "does not read"));
    let mut wrong = d.clone();
    wrong["bindings"]["$concat"][1]["$flatten"]["$map"][2][0]["source"] = json!("packed.0");
    cases.push((wrong, "roles only"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["unused"] = nibble_descriptor();
    cases.push((wrong, "unused format"));
    for (wrong, why) in cases {
        let e = err(&wrong, &config(), &s);
        assert!(e.contains(why), "wanted {why}, got {e}");
    }
    s.tensors.get_mut("packed.0").unwrap().0.bytes = 9;
    assert!(err(&d, &config(), &s).contains("source byte count"));
    assert_eq!(s.reads.get(), 0);
}

#[test]
fn packed_source_changes_during_conversion_refuse_and_preserve_the_previous_output() {
    struct Changed<'a> {
        base: &'a Source,
        calls: Cell<usize>,
    }
    impl TensorSource for Changed<'_> {
        fn names(&self) -> Vec<String> {
            self.base.names()
        }
        fn shape(&self, n: &str) -> Option<Vec<usize>> {
            self.base.shape(n)
        }
        fn metadata(&self, n: &str) -> Option<TensorMeta> {
            self.base.metadata(n)
        }
        fn load(&self, _: &str) -> misaka_palw_tir_lower::Result<misaka_palw_tir_lower::weights::Tensor> {
            panic!("whole loading")
        }
        fn read_slice(&self, n: &str, r: Range<u64>) -> misaka_palw_tir_lower::Result<Vec<u8>> {
            let mut bytes = self.base.read_slice(n, r)?;
            if n == "packed.0" {
                self.calls.set(self.calls.get() + 1);
                if self.calls.get() >= 2 {
                    bytes[0] ^= 1;
                }
            }
            Ok(bytes)
        }
    }
    let (_, d, s, _) = packed_case();
    let compiled = FrontendPack::parse(&d.to_string()).unwrap().compile(&config(), &s, &admission::default_inputs()).unwrap();
    let changed = Changed { base: &s, calls: Cell::new(0) };
    let dir = Temp::new();
    let path = dir.0.join("artifact");
    std::fs::write(&path, b"previous").unwrap();
    let error = compiled.write(&path, &changed, [0; 64], 16).err().unwrap().to_string();
    assert!(error.contains("source changed during conversion"), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), b"previous");
    assert!(std::fs::read_dir(&dir.0).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
}

#[test]
fn descriptor_nested_config_consumes_only_the_declared_leaves() {
    let (_, mut d, s, _) = packed_case();
    d["quant_formats"]["outsider"]["params"]["bias"]["config"] = json!("group.bias");
    d["quant_formats"]["outsider"]["tests"][0]["config"] = json!({"group":{"bias":8}});
    d["bindings"]["$concat"][1]["$flatten"]["$map"][2][0]["import"]["config"] = json!({"group":{"bias":8}});
    FrontendPack::parse(&d.to_string()).unwrap().compile(&config(), &s, &admission::default_inputs()).unwrap();
    let mut wrong = d.clone();
    wrong["bindings"]["$concat"][1]["$flatten"]["$map"][2][0]["import"]["config"]["group"]["future"] = json!(1);
    assert!(err(&wrong, &config(), &s).contains("group.future does not read"));
    let mut wrong = d.clone();
    wrong["quant_formats"]["outsider"]["params"]["bias"]["config"] = json!("group.bias[no-index]");
    assert!(err(&wrong, &config(), &s).contains("invalid config path"));
    d["quant_formats"]["outsider"]["params"]["bias"]["config"] = json!("group[0]");
    d["quant_formats"]["outsider"]["tests"][0]["config"] = json!({"group":[8]});
    d["bindings"]["$concat"][1]["$flatten"]["$map"][2][0]["import"]["config"] = json!({"group":[8]});
    FrontendPack::parse(&d.to_string()).unwrap().compile(&config(), &s, &admission::default_inputs()).unwrap();
    d["bindings"]["$concat"][1]["$flatten"]["$map"][2][0]["import"]["config"] = json!({"group":[8,9]});
    assert!(err(&d, &config(), &s).contains("group[1] does not read"));
    assert_eq!(s.reads.get(), 0);
}

#[test]
fn lane_indexed_header_dimensions_do_not_depend_on_stream_block_boundaries() {
    let (p, mut d, s, _) = packed_case();
    d["quant_formats"]["outsider"]["decode"]["value"] = json!("(((codes[o,i/2] >> (4*(i%2))) & 15) - bias) * dim_codes[i%2]");
    let expected: Vec<u8> = [-8f32, 14.0, 0.0, -2.0].into_iter().flat_map(f32::to_le_bytes).collect();
    d["quant_formats"]["outsider"]["tests"][0]["values_f32_hex"] = json!(frontend_pack::program::hex(&expected));
    let c = FrontendPack::parse(&d.to_string()).unwrap().compile(&config(), &s, &admission::default_inputs()).unwrap();
    let dir = Temp::new();
    let a = dir.0.join("a");
    let b = dir.0.join("b");
    let output = c.write(&a, &s, [0; 64], 8).unwrap();
    c.write_checked(&b, &s, [0; 64], 31, Some(&output.record)).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    let (_, params) = artifact::read(&a, &p).unwrap();
    let raw = &s.tensors["packed.0"].1;
    let want: Vec<u8> =
        (0..16).map(|i| ((((raw[i / 2] >> (4 * (i % 2))) & 15) as i8 - 8) * if i % 2 == 0 { 4 } else { 2 }) as u8).collect();
    assert_eq!(params.tensors[&(1, Some(0))].le_bytes(), want);
}

#[test]
fn the_pack_wide_descriptor_vector_budget_cannot_reset_between_formats() {
    let (_, mut d, s, _) = packed_case();
    let mut descriptor = nibble_descriptor();
    descriptor["decode"]["value"] = json!("((codes[o,i/2] >> (4*(i%2))) & 15) - bias + bias - bias");
    let raw = "f0".repeat(4096);
    let values: Vec<u8> = (0..8192).flat_map(|i| if i % 2 == 0 { (-8f32).to_le_bytes() } else { 7f32.to_le_bytes() }).collect();
    let vector = json!({"roles":{"codes":{"dtype":"U8","shape":[4096,1],"hex":raw}},
        "config":{"bias":8},"values_f32_hex":frontend_pack::program::hex(&values)});
    descriptor["tests"] = json!([vector.clone(), vector.clone(), vector]);
    let formats: serde_json::Map<String, Value> = (0..9).map(|i| (format!("format-{i}"), descriptor.clone())).collect();
    d["quant_formats"] = Value::Object(formats);
    let text = d.to_string();
    assert!(text.len() < frontend_pack::MAX_PACK_BYTES, "must reach the vector work bound");
    let error = FrontendPack::parse(&text).err().unwrap().to_string();
    assert!(error.contains("FRONTEND_DESCRIPTOR_LIMIT: vector work"), "{error}");
    assert_eq!(s.reads.get(), 0);
}
