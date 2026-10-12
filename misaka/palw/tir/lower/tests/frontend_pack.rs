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
    wrong["quant_formats"]["outsider"]["decode"]["value"] = json!("1+".repeat(160) + "1");
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
    wrong["quant_formats"]["outsider"]["params"] = json!({"unused":{"default":1}});
    cases.push((wrong, "unsupported block configuration"));
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

fn saved_fixture(f: &misaka_palw_tir_lower::quantfmt::QuantFormat, vector: usize) -> (Value, Source, TirProgramV1, Vec<i128>) {
    use misaka_palw_tir_lower::quantfmt::tensors::RoleTensor;
    let test = &f.desc.tests[vector];
    let mut tensors = BTreeMap::new();
    let mut roles = serde_json::Map::new();
    let (out, inp) = if let Some(t) = f.as_tensors() {
        let mut headers = Vec::new();
        for (name, _, _) in t.roles() {
            if let Some(r) = test.roles.get(name) {
                let raw = frontend_pack::program::unhex(&r.hex, 64 << 10).unwrap();
                headers.push(Some(RoleTensor { shape: r.shape.clone(), dtype: r.dtype.clone(), data: raw.clone() }));
                let key = format!("outside.{name}");
                roles.insert(name.into(), json!(key));
                tensors.insert(key, (TensorMeta { shape: r.shape.clone(), dtype: r.dtype.clone(), bytes: raw.len() as u64 }, raw));
            } else {
                headers.push(None);
            }
        }
        let params = f.read_config(&test.config.clone().unwrap_or(json!({}))).unwrap().params;
        let shape = t.prepare_streamed(&headers, &params).unwrap().shape();
        (shape[0], shape[1])
    } else {
        let b = f.as_blocks().unwrap();
        let raw = frontend_pack::program::unhex(&test.block_hex, 64 << 10).unwrap();
        let total = raw.len() / b.bytes * b.elems;
        tensors
            .insert("outside.data".into(), (TensorMeta { dtype: "U8".into(), shape: vec![raw.len()], bytes: raw.len() as u64 }, raw));
        roles.insert("data".into(), json!("outside.data"));
        (1, total)
    };
    let mut pb = ProgramBuilder::new(out as u32, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("public.storage", DType::I16, &[out as u32, inp as u32], false);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let y = b.gather(table, Ref::Input(0), 0, 0);
        let y = b.cast(y, DType::I32);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[inp as u32])]);
        let y = b.reshape_fixed(Ref::CarryIn(0), &[inp as u32, 1]);
        let y = b.matmul(table, y, DType::I64);
        let y = b.reshape_fixed(y, &[out as u32]);
        let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(y);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let p = pb.finish(pre, vec![], post, logits);
    let mut inert = serde_json::Map::new();
    // The library writes a type label; the descriptor reads block size/dtype/shape/nesting.
    if test.roles.contains_key("qstate") {
        inert.insert("qstate".into(), json!(["quant_type"]));
    }
    let d = json!({"format":frontend_pack::FORMAT,"id":"independent-saved-layout","scope":{"task":"text-generation","completeness":"partial","components":["stored-weight-fixture"]},
        "program":Program::of(&p),"quant_formats":{"published":f.desc},"bindings":[
        {"param":0,"layer":null,"import":{"kind":"descriptor","format":"published","roles":roles,
         "config":test.config.clone().unwrap_or(json!({})),"metadata_inert":inert,"shift":0,"round":"half_away_from_zero","overflow":"saturate"}}]});
    let raw = frontend_pack::program::unhex(&test.values_f32_hex, 64 << 10).unwrap();
    let expected = raw
        .chunks_exact(4)
        .map(|b| {
            let f = f32::from_le_bytes(b.try_into().unwrap());
            assert!(f.is_finite());
            (f.round() as i128).clamp(i16::MIN as i128, i16::MAX as i128)
        })
        .collect();
    (d, Source { tensors, reads: Cell::new(0), peak: Cell::new(0), limit: 256 }, p, expected)
}

#[test]
fn every_builtin_tensors_and_blocks_layout_imports_without_registry_dispatch() {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let dir = Temp::new();
    let mut count = 0;
    for f in reg.all().iter().filter(|f| f.as_virtual().is_none()) {
        let (d, s, p, expected) = saved_fixture(f, 0);
        let pack = FrontendPack::parse(&d.to_string()).unwrap_or_else(|e| panic!("{}: {e}", f.name()));
        let compiled =
            pack.compile_bounded(&json!({}), &s, &admission::default_inputs(), 256).unwrap_or_else(|e| panic!("{}: {e}", f.name()));
        let path = dir.0.join(f.name());
        let conversion = compiled.write(&path, &s, [0; 64], 256).unwrap_or_else(|e| panic!("{}: {e}", f.name()));
        assert!(conversion.max_read_bytes <= 256);
        let (_, params) = artifact::read(&path, &p).unwrap();
        assert_eq!(
            params.tensors[&(0, None)].le_bytes(),
            expected.into_iter().flat_map(|n| (n as i16).to_le_bytes()).collect::<Vec<_>>(),
            "{}",
            f.name()
        );
        common::three_ways(&p, &params, &[vec![0, 0]]).unwrap();
        common::court_coverage(&p, &params, &[0, 0], &[0, 1], &[4]).unwrap();
        count += 1;
    }
    assert_eq!(count, 40);
}

#[test]
fn role_json_and_shape_metadata_are_read_bounded_and_pinned_before_artifact_write() {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let dir = Temp::new();
    for name in ["BNB_NF4", "CT_PACK_QUANTIZED"] {
        let f = reg.all().iter().find(|f| f.name() == name).unwrap();
        let (d, mut s, p, _) = saved_fixture(f, 0);
        s.limit = 48;
        let pack = FrontendPack::parse(&d.to_string()).unwrap();
        let compiled = pack.compile_bounded(&json!({}), &s, &admission::default_inputs(), 48).unwrap();
        assert!(s.reads.get() > 0);
        assert!(s.peak.get() <= 48);
        let a = dir.0.join(format!("{name}-a"));
        let first = compiled.write(&a, &s, [0; 64], 48).unwrap();
        s.limit = 96;
        let other = pack.compile_bounded(&json!({}), &s, &admission::default_inputs(), 96).unwrap();
        let b = dir.0.join(format!("{name}-b"));
        let second = other.write_checked(&b, &s, [0; 64], 96, Some(&first.record)).unwrap();
        assert_eq!(first.record, second.record);
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
        let metadata = if name == "BNB_NF4" { "outside.qstate" } else { "outside.shape" };
        let raw = &mut s.tensors.get_mut(metadata).unwrap().1;
        raw[0] ^= 1;
        let previous = std::fs::read(&b).unwrap();
        let e = compiled.write(&b, &s, [0; 64], 48).err().unwrap().to_string();
        assert!(e.contains("metadata changed since compilation"), "{e}");
        assert_eq!(std::fs::read(&b).unwrap(), previous);
        assert_eq!(compiled.program(), &p);
    }
}

#[test]
fn unread_role_json_unknown_inert_metadata_and_later_bad_headers_refuse() {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let f = reg.all().iter().find(|f| f.name() == "BNB_NF4").unwrap();
    let (d, s, _, _) = saved_fixture(f, 0);
    let mut wrong = d.clone();
    wrong["bindings"][0]["import"]["metadata_inert"] = json!({});
    let e = refuse_saved(wrong, &json!({}), &s);
    assert!(e.contains("quant_type"), "{e}");
    let mut wrong = d.clone();
    wrong["bindings"][0]["import"]["metadata_inert"] = json!({"weight":["anything"]});
    let e = refuse_saved(wrong, &json!({}), &s);
    assert!(e.contains("declared JSON role"), "{e}");
    let mut wrong = d.clone();
    wrong["bindings"][0]["import"]["roles"]["extra"] = json!("outside.qstate");
    s.reads.set(0);
    assert!(refuse_saved(wrong, &json!({}), &s).contains("undeclared descriptor role"));
    assert_eq!(s.reads.get(), 0);
    let mut wrong = d.clone();
    wrong["bindings"] = json!([]);
    s.reads.set(0);
    assert!(refuse_saved(wrong, &json!({}), &s).contains("missing parameter instance"));
    assert_eq!(s.reads.get(), 0);
    let mut wrong = d;
    wrong["bindings"][0]["param"] = json!(1);
    s.reads.set(0);
    assert!(refuse_saved(wrong, &json!({}), &s).contains("unexpected"));
    assert_eq!(s.reads.get(), 0);
}

fn refuse_saved(d: Value, config: &Value, source: &Source) -> String {
    match FrontendPack::parse(&d.to_string())
        .and_then(|p| p.compile_bounded(config, source, &admission::default_inputs(), source.limit))
    {
        Err(e) => e.to_string(),
        Ok(_) => panic!("malformed saved format was admitted"),
    }
}

#[test]
fn unaligned_block_fields_and_opaque_storage_use_bounded_pages_with_the_exact_pinned_layout() {
    let raw: Vec<_> = [1.25f64, -2.5, 3.75, 4.0].into_iter().flat_map(|f| std::iter::once(0x99).chain(f.to_le_bytes())).collect();
    let expected: Vec<_> = [1.25f32, -2.5, 3.75, 4.0].into_iter().flat_map(f32::to_le_bytes).collect();
    let f = misaka_palw_tir_lower::quantfmt::QuantFormat::from_json(&json!({
        "schema":"misaka.palw.quant-format.v1","name":"outside-unaligned-double-block",
        "layout":{"kind":"blocks","elems":1,"bytes":9,"fields":[{"name":"value","at":1,"type":"f64"}]},
        "decode":{"target":"floats","value":"value"},"tests":[{"block_hex":frontend_pack::program::hex(&raw),"values_f32_hex":frontend_pack::program::hex(&expected)}]
    }).to_string()).unwrap();
    let (d, mut s, p, expected) = saved_fixture(&f, 0);
    s.limit = 8;
    // A raw GGUF-like source gives a logical shape and an opaque dtype. No type-name lookup occurs.
    let meta = &mut s.tensors.get_mut("outside.data").unwrap().0;
    meta.dtype = "UnregisteredStoredType999".into();
    meta.shape = vec![1, 4];
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let compiled = pack.compile_bounded(&json!({}), &s, &admission::default_inputs(), 8).unwrap();
    let dir = Temp::new();
    let path = dir.0.join("unaligned");
    let conversion = compiled.write(&path, &s, [0; 64], 8).unwrap();
    assert!(conversion.max_read_bytes <= 8);
    let (_, params) = artifact::read(&path, &p).unwrap();
    assert_eq!(params.tensors[&(0, None)].le_bytes(), expected.into_iter().flat_map(|n| (n as i16).to_le_bytes()).collect::<Vec<_>>());
    let mut wrong = s.tensors.clone();
    wrong.get_mut("outside.data").unwrap().0.bytes -= 1;
    let wrong = Source { tensors: wrong, reads: Cell::new(0), peak: Cell::new(0), limit: 8 };
    assert!(refuse_saved(d, &json!({}), &wrong).contains("block source bytes/shape/storage"));
    assert_eq!(wrong.reads.get(), 0);
}

#[test]
fn stored_format_metadata_limits_and_storage_semantics_refuse_before_weight_reads() {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let f = reg.all().iter().find(|f| f.name() == "BNB_NF4").unwrap();
    let (d, mut s, _, _) = saved_fixture(f, 0);
    s.tensors.get_mut("outside.qstate").unwrap().0 = TensorMeta { dtype: "U8".into(), shape: vec![65537], bytes: 65537 };
    assert!(refuse_saved(d, &json!({}), &s).contains("role metadata exceeds 64 KiB"));
    assert_eq!(s.reads.get(), 0);
    let f = reg.all().iter().find(|f| f.name() == "AWQ").unwrap();
    for (key, v) in [("version", json!("gemv")), ("zero_point", json!(false)), ("desc_act", json!(true))] {
        let (mut d, s, _, _) = saved_fixture(f, 0);
        d["bindings"][0]["import"]["config"][key] = v;
        assert!(refuse_saved(d, &json!({}), &s).contains("AWQ"));
        assert_eq!(s.reads.get(), 0);
    }
    let f = reg.all().iter().find(|f| f.name() == "CT_PACK_QUANTIZED").unwrap();
    let (mut d, s, _, _) = saved_fixture(f, 0);
    d["bindings"][0]["import"]["config"]["config_groups"]["group_0"]["input_activations"] = json!({"num_bits":8});
    assert!(refuse_saved(d, &json!({}), &s).contains("activations"));
    assert_eq!(s.reads.get(), 0);
}

#[test]
fn frontend_cli_rebuilds_role_json_metadata_with_the_chosen_range_budget() {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let f = reg.all().iter().find(|f| f.name() == "BNB_NF4").unwrap();
    let (d, mut s, _, _) = saved_fixture(f, 0);
    s.limit = 32;
    let dir = Temp::new();
    let mut header = serde_json::Map::new();
    let mut bytes: Vec<u8> = Vec::new();
    for (name, (meta, raw)) in &s.tensors {
        let start = bytes.len();
        bytes.extend(raw);
        header.insert(name.clone(), json!({"dtype":meta.dtype,"shape":meta.shape,"data_offsets":[start,bytes.len()]}));
    }
    let encoded = serde_json::to_vec(&header).unwrap();
    let mut file = (encoded.len() as u64).to_le_bytes().to_vec();
    file.extend(encoded);
    file.extend(bytes);
    std::fs::write(dir.0.join("model.safetensors"), file).unwrap();
    std::fs::write(dir.0.join("config.json"), "{}").unwrap();
    let pack_path = dir.0.join("frontend.json");
    std::fs::write(&pack_path, d.to_string()).unwrap();
    let direct = dir.0.join("direct");
    let built = FrontendPack::parse(&d.to_string())
        .unwrap()
        .compile_bounded(&json!({}), &s, &admission::default_inputs(), 32)
        .unwrap()
        .write(&direct, &s, [0; 64], 32)
        .unwrap();
    let cli = dir.0.join("cli");
    let receipt = dir.0.join("record.json");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_palw-tir-frontend"))
        .arg(&dir.0)
        .arg("--frontend-pack")
        .arg(&pack_path)
        .arg("--out")
        .arg(&cli)
        .arg("--record")
        .arg(&receipt)
        .args(["--block-bytes", "32"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let record: frontend_pack::BuildRecord = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
    assert_eq!(record, built.record);
    assert_eq!(std::fs::read(cli).unwrap(), std::fs::read(direct).unwrap());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["max_read_bytes"].as_u64().unwrap() <= 32);
}

#[test]
fn a_nested_inert_storage_path_consumes_its_leaf_without_authorizing_other_siblings() {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let f = reg.all().iter().find(|f| f.name() == "AWQ").unwrap();
    let (mut d, s, _, _) = saved_fixture(f, 0);
    d["quant_formats"]["published"]["config"]["inert"] = json!(["tooling.author"]);
    d["bindings"][0]["import"]["config"]["tooling"] = json!({"author":"independent"});
    FrontendPack::parse(&d.to_string()).unwrap().compile_bounded(&json!({}), &s, &admission::default_inputs(), 256).unwrap();
    assert_eq!(s.reads.get(), 0);
    d["bindings"][0]["import"]["config"]["tooling"]["arithmetic"] = json!("different");
    assert!(refuse_saved(d, &json!({}), &s).contains("tooling.arithmetic"));
    assert_eq!(s.reads.get(), 0);
}

#[test]
fn aggregate_metadata_is_reserved_before_reading_any_repeated_binding() {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("table", DType::I8, &[16, 4], false);
    let a = pb.param("a", DType::I8, &[4, 4], true);
    let b = pb.param("b", DType::I8, &[4, 4], true);
    let pre = {
        let mut block = pb.block("pre", vec![]);
        let y = block.gather(table, Ref::Input(0), 0, 0);
        let y = block.cast(y, DType::I32);
        block.finish(&[y])
    };
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let layer = {
        let mut block = pb.block("layer", carry.clone());
        let x = block.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let x = block.matmul(a, x, DType::I64);
        let x = block.clamp(x, -1000, 1000, DType::I32);
        let x = block.matmul(b, x, DType::I64);
        let x = block.clamp(x, -1000, 1000, DType::I32);
        let x = block.reshape_fixed(x, &[4]);
        block.finish(&[x])
    };
    let post = {
        let mut block = pb.block("post", carry);
        let x = block.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let y = block.matmul(table, x, DType::I64);
        let y = block.reshape_fixed(y, &[16]);
        let y = block.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
        block.commit(y);
        block.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let p = pb.finish(pre, vec![layer; 600], post, logits);
    let floats = vec![0u8; 64];
    let descriptor = json!({"schema":"misaka.palw.quant-format.v1","name":"large-shared-metadata",
        "layout":{"kind":"tensors","roles":[{"name":"q","rank":2,"dtypes":["I8"],"suffix":".q"},{"name":"marker","rank":1,"dtypes":["U8"],"suffix":".marker"}],
        "dims":{"out":"dim_q[0]","inp":"dim_q[1]"},"checks":[{"expr":"marker[0] == 0","message":"marker mismatch"}]},
        "decode":{"target":"floats","value":"q[o,i]"},"tests":[{"roles":{"q":{"dtype":"I8","shape":[4,4],"hex":"00".repeat(16)},"marker":{"dtype":"U8","shape":[1],"hex":"00"}},"values_f32_hex":frontend_pack::program::hex(&floats)}]});
    let mut bindings = vec![json!({"param":0,"layer":null,"source":"table","import":{"kind":"integer"}})];
    for layer in 0..600 {
        for param in [1, 2] {
            bindings.push(json!({"param":param,"layer":layer,"import":{"kind":"descriptor","format":"public","roles":{"q":"q","marker":"marker"},"config":{},"shift":0,"round":"half_away_from_zero","overflow":"reject"}}));
        }
    }
    let d = json!({"format":frontend_pack::FORMAT,"id":"aggregate-metadata","scope":{"task":"text-generation","completeness":"partial","components":["metadata-budget"]},"program":Program::of(&p),"quant_formats":{"public":descriptor},"bindings":bindings});
    let s = Source {
        tensors: BTreeMap::from([
            ("table".into(), (TensorMeta { dtype: "I8".into(), shape: vec![16, 4], bytes: 64 }, vec![0; 64])),
            ("q".into(), (TensorMeta { dtype: "I8".into(), shape: vec![4, 4], bytes: 16 }, vec![0; 16])),
            ("marker".into(), (TensorMeta { dtype: "U8".into(), shape: vec![65536], bytes: 65536 }, vec![])),
        ]),
        reads: Cell::new(0),
        peak: Cell::new(0),
        limit: 65536,
    };
    let e = refuse_saved(d, &json!({}), &s);
    assert!(e.contains("aggregate metadata bytes"), "{e}");
    assert_eq!(s.reads.get(), 0);
}

type GgufStored = (String, Vec<usize>, u32, Vec<u8>);
fn gguf_string(bytes: &mut Vec<u8>, text: &str) {
    bytes.extend((text.len() as u64).to_le_bytes());
    bytes.extend(text.as_bytes());
}
fn native_gguf(tensors: &[GgufStored], metadata: &[(String, u32, Vec<u8>)], offsets: Option<&[u64]>) -> Vec<u8> {
    let mut bytes = b"GGUF".to_vec();
    bytes.extend(3u32.to_le_bytes());
    bytes.extend((tensors.len() as u64).to_le_bytes());
    bytes.extend((metadata.len() as u64).to_le_bytes());
    for (key, ty, raw) in metadata {
        gguf_string(&mut bytes, key);
        bytes.extend(ty.to_le_bytes());
        bytes.extend(raw);
    }
    let mut data = Vec::new();
    for (i, (name, shape, ty, raw)) in tensors.iter().enumerate() {
        data.resize(data.len().div_ceil(32) * 32, 0);
        gguf_string(&mut bytes, name);
        bytes.extend((shape.len() as u32).to_le_bytes());
        for dim in shape.iter().rev() {
            bytes.extend((*dim as u64).to_le_bytes());
        }
        bytes.extend(ty.to_le_bytes());
        bytes.extend(offsets.map_or(data.len() as u64, |o| o[i]).to_le_bytes());
        data.extend(raw);
    }
    bytes.resize(bytes.len().div_ceil(32) * 32, 0);
    bytes.extend(data);
    bytes
}
fn gguf_meta() -> Vec<(String, u32, Vec<u8>)> {
    let mut raw = Vec::new();
    gguf_string(&mut raw, "never-registered-architecture");
    vec![("general.architecture".into(), 8, raw)]
}
fn gguf_tokenizer_meta() -> (String, u32, Vec<u8>) {
    let mut raw = Vec::new();
    raw.extend(8u32.to_le_bytes());
    raw.extend(2u64.to_le_bytes());
    for token in ["Hello", "世界"] {
        gguf_string(&mut raw, token);
    }
    ("tokenizer.ggml.tokens".into(), 9, raw)
}

#[test]
fn native_gguf_imports_all_block_formats_by_explicit_binding_and_retains_exact_execution() {
    use frontend_pack::FrontendSource;
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let dir = Temp::new();
    let mut count = 0;
    for f in reg.all().iter().filter(|f| f.as_blocks().is_some()) {
        let (mut d, s, p, expected) = saved_fixture(f, 0);
        d["inert"] = json!(["general.architecture"]);
        let pack = FrontendPack::parse(&d.to_string()).unwrap();
        let path = dir.0.join(format!("{}.gguf", f.name()));
        let shape = p.params[0].shape.iter().map(|n| *n as usize).collect();
        std::fs::write(
            &path,
            native_gguf(
                &[("outside.data".into(), shape, f.ggml_id().unwrap(), s.tensors["outside.data"].1.clone())],
                &gguf_meta(),
                None,
            ),
        )
        .unwrap();
        let source = FrontendSource::open(&path, &pack).unwrap_or_else(|e| panic!("{}: {e}", f.name()));
        let config = source.configuration(json!({})).unwrap();
        let compiled = pack.compile_bounded(&config, &source, &admission::default_inputs(), 8).unwrap();
        let artifact_path = dir.0.join(f.name());
        let a = compiled.write(&artifact_path, &source, [0; 64], 8).unwrap();
        assert!(a.max_read_bytes <= 8);
        let peer = dir.0.join(format!("{}-peer", f.name()));
        let b = compiled.write_checked(&peer, &source, [0; 64], 127, Some(&a.record)).unwrap();
        assert_eq!(a.record, b.record);
        assert_eq!(std::fs::read(&artifact_path).unwrap(), std::fs::read(peer).unwrap());
        let (_, params) = artifact::read(&artifact_path, &p).unwrap();
        assert_eq!(
            params.tensors[&(0, None)].le_bytes(),
            expected.into_iter().flat_map(|v| (v as i16).to_le_bytes()).collect::<Vec<_>>(),
            "{}",
            f.name()
        );
        common::three_ways(&p, &params, &[vec![0, 0]]).unwrap();
        common::court_coverage(&p, &params, &[0, 0], &[0, 1], &[4]).unwrap();
        count += 1;
    }
    assert_eq!(count, 31);
}

#[test]
fn native_gguf_scalar_integers_preserve_i64_bits_without_family_or_float_loading() {
    use frontend_pack::FrontendSource;
    let dir = Temp::new();
    let p = program();
    let s = source(&p);
    let mut d = definition(&p);
    d["inert"].as_array_mut().unwrap().push(json!("general.architecture"));
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let tensors: Vec<_> = s
        .tensors
        .iter()
        .map(|(n, (m, raw))| {
            let ty = match m.dtype.as_str() {
                "I8" => 24,
                "I16" => 25,
                "I64" => 27,
                _ => panic!(),
            };
            (n.clone(), m.shape.clone(), ty, raw.clone())
        })
        .collect();
    let path = dir.0.join("model.gguf");
    std::fs::write(&path, native_gguf(&tensors, &gguf_meta(), None)).unwrap();
    let source = FrontendSource::open(&dir.0, &pack).unwrap();
    let config = source.configuration(config()).unwrap();
    let compiled = pack.compile_bounded(&config, &source, &admission::default_inputs(), 8).unwrap();
    let artifact_path = dir.0.join("native");
    compiled.write(&artifact_path, &source, [0; 64], 8).unwrap();
    let (_, params) = artifact::read(&artifact_path, &p).unwrap();
    for ((j, l), tensor) in &params.tensors {
        let name = match j {
            0 => "stranger.table".into(),
            1 => format!("stranger.w.{}", l.unwrap()),
            2 => format!("stranger.m.{}", l.unwrap()),
            3 => "stranger.head".into(),
            _ => panic!(),
        };
        assert_eq!(tensor.le_bytes(), s.tensors[&name].1);
    }
    common::three_ways(&p, &params, &[vec![1, 2, 3, 4]]).unwrap();
    common::court_coverage(&p, &params, &[1, 2, 3, 4], &[0, 1, 3], &[4]).unwrap();
}

fn stranger_gguf_descriptor() -> misaka_palw_tir_lower::quantfmt::QuantFormat {
    let floats: Vec<_> = [-8f32, 7.0, 0.0, -1.0].into_iter().flat_map(f32::to_le_bytes).collect();
    misaka_palw_tir_lower::quantfmt::QuantFormat::from_json(&json!({"schema":"misaka.palw.quant-format.v1","name":"NeverSeenByThisBuild",
        "ids":[{"scheme":"ggml","id":65535}],"layout":{"kind":"blocks","elems":4,"bytes":2,"fields":[{"name":"codes","at":0,"type":"u8","count":2}]},
        "decode":{"target":"integers","group":{"size":4},"q":"(codes[e/2] >> (4*(e%2))) & 15","scale":"1","zero":"8","code":{"min":0,"max":15}},
        "tests":[{"block_hex":"f078","values_f32_hex":frontend_pack::program::hex(&floats)}]}).to_string()).unwrap()
}

#[test]
fn native_gguf_storage_type_does_not_select_integer_arithmetic_or_decoder() {
    use frontend_pack::FrontendSource;
    let dir = Temp::new();
    let (mut d, _, p, _) = saved_fixture(&stranger_gguf_descriptor(), 0);
    let floats = [f64::from_bits(0.5f64.to_bits() - 1), 0.5, 1.5, -1.5];
    let raw: Vec<_> = floats.into_iter().flat_map(f64::to_le_bytes).collect();
    let path = dir.0.join("same-bytes.gguf");
    std::fs::write(&path, native_gguf(&[("outside.data".into(), vec![1, 4], 28, raw)], &[], None)).unwrap();
    d.as_object_mut().unwrap().remove("quant_formats");
    d["bindings"][0] = json!({"param":0,"layer":null,"source":"outside.data","import":{"kind":"fixed_point","shift":0,"round":"half_away_from_zero","overflow":"reject"}});
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let source = FrontendSource::open(&path, &pack).unwrap();
    let config = source.configuration(json!({})).unwrap();
    let compiled = pack.compile_bounded(&config, &source, &admission::default_inputs(), 8).unwrap();
    let target = dir.0.join("exact-ieee");
    let a = compiled.write(&target, &source, [0; 64], 8).unwrap();
    let (_, params) = artifact::read(&target, &p).unwrap();
    assert_eq!(params.tensors[&(0, None)].le_bytes(), [0i16, 1, 2, -2].into_iter().flat_map(i16::to_le_bytes).collect::<Vec<_>>());
    // This binding explicitly requests binary32 between storage decoding and integer rounding.
    let f = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin().ggml(28).unwrap();
    d["quant_formats"] = json!({"declared":f.desc});
    d["bindings"][0] = json!({"param":0,"layer":null,"import":{"kind":"descriptor","format":"declared","roles":{"data":"outside.data"},"config":{},"shift":0,"round":"half_away_from_zero","overflow":"reject"}});
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let source = FrontendSource::open(&path, &pack).unwrap();
    let compiled = pack.compile_bounded(&config, &source, &admission::default_inputs(), 8).unwrap();
    let target = dir.0.join("declared-binary32");
    let b = compiled.write(&target, &source, [0; 64], 8).unwrap();
    let (_, params) = artifact::read(&target, &p).unwrap();
    assert_eq!(params.tensors[&(0, None)].le_bytes(), [1i16, 1, 2, -2].into_iter().flat_map(i16::to_le_bytes).collect::<Vec<_>>());
    assert_ne!(a.record.artifact_digest, b.record.artifact_digest);
}

#[test]
fn native_gguf_header_change_during_weight_reads_refuses_before_output_replacement() {
    use frontend_pack::FrontendSource;
    struct MutatingHeader {
        source: FrontendSource,
        path: PathBuf,
        replacement: Vec<u8>,
        changed: Cell<bool>,
    }
    impl TensorSource for MutatingHeader {
        fn names(&self) -> Vec<String> {
            self.source.names()
        }
        fn shape(&self, n: &str) -> Option<Vec<usize>> {
            self.source.shape(n)
        }
        fn metadata(&self, n: &str) -> Option<TensorMeta> {
            self.source.metadata(n)
        }
        fn load(&self, _: &str) -> misaka_palw_tir_lower::Result<misaka_palw_tir_lower::weights::Tensor> {
            panic!("no whole loading")
        }
        fn validate_snapshot(&self) -> misaka_palw_tir_lower::Result<()> {
            self.source.validate_snapshot()
        }
        fn read_slice(&self, n: &str, r: Range<u64>) -> misaka_palw_tir_lower::Result<Vec<u8>> {
            let bytes = self.source.read_slice(n, r)?;
            if !self.changed.replace(true) {
                std::fs::write(&self.path, &self.replacement).unwrap();
            }
            Ok(bytes)
        }
    }
    let dir = Temp::new();
    let (mut d, s, _, _) = saved_fixture(&stranger_gguf_descriptor(), 0);
    d["inert"] = json!(["general.architecture"]);
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let tensors = vec![("outside.data".into(), vec![1, 4], 65535, s.tensors["outside.data"].1.clone())];
    let path = dir.0.join("mutable.gguf");
    let meta = gguf_meta();
    std::fs::write(&path, native_gguf(&tensors, &meta, None)).unwrap();
    let source = FrontendSource::open(&path, &pack).unwrap();
    let config = source.configuration(json!({})).unwrap();
    let compiled = pack.compile_bounded(&config, &source, &admission::default_inputs(), 8).unwrap();
    let mut meta = meta;
    *meta[0].2.last_mut().unwrap() ^= 1;
    let mutator = MutatingHeader { source, path, replacement: native_gguf(&tensors, &meta, None), changed: Cell::new(false) };
    let target = dir.0.join("existing");
    std::fs::write(&target, b"previous artifact").unwrap();
    let e = compiled.write(&target, &mutator, [0; 64], 8).err().unwrap().to_string();
    assert!(mutator.changed.get());
    assert!(e.contains("header changed"), "{e}");
    assert_eq!(std::fs::read(&target).unwrap(), b"previous artifact");
}

#[test]
fn native_gguf_unknown_type_rebuilds_via_cli_and_refuses_unread_or_changed_metadata() {
    use frontend_pack::FrontendSource;
    let dir = Temp::new();
    let f = stranger_gguf_descriptor();
    let (mut d, s, p, _) = saved_fixture(&f, 0);
    let tensors = vec![("outside.data".into(), vec![1, 4], 65535, s.tensors["outside.data"].1.clone())];
    let path = dir.0.join("third-party.gguf");
    let mut metadata = gguf_meta();
    metadata.push(gguf_tokenizer_meta());
    let raw = native_gguf(&tensors, &metadata, None);
    std::fs::write(&path, &raw).unwrap();
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let source = FrontendSource::open(&path, &pack).unwrap();
    let config = source.configuration(json!({})).unwrap();
    assert!(pack.compile(&config, &source, &admission::default_inputs()).err().unwrap().to_string().contains("general.architecture"));
    assert!(source.configuration(json!({"general.architecture":"hidden"})).unwrap_err().to_string().contains("duplicates native key"));
    d["inert"] = json!(["general.architecture", "tokenizer.ggml.tokens"]);
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let compiled = pack.compile_bounded(&config, &source, &admission::default_inputs(), 8).unwrap();
    let target = dir.0.join("existing");
    let tokenizer = source.embedded_tokenizer_id().unwrap().unwrap();
    assert_ne!(tokenizer, [0; 64]);
    let c = compiled.write(&target, &source, tokenizer, 8).unwrap();
    let frontend = dir.0.join("frontend.json");
    let receipt = dir.0.join("receipt.json");
    std::fs::write(&frontend, d.to_string()).unwrap();
    std::fs::write(&receipt, serde_json::to_vec(&c.record).unwrap()).unwrap();
    let cli_target = dir.0.join("cli");
    let r = std::process::Command::new(env!("CARGO_BIN_EXE_palw-tir-frontend"))
        .arg(&dir.0)
        .arg("--frontend-pack")
        .arg(&frontend)
        .arg("--out")
        .arg(&cli_target)
        .arg("--record")
        .arg(dir.0.join("cli.json"))
        .arg("--expect-record")
        .arg(&receipt)
        .arg("--block-bytes")
        .arg("127")
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(std::fs::read(&target).unwrap(), std::fs::read(&cli_target).unwrap());
    let external = dir.0.join("explicit-tokenizer.json");
    std::fs::write(&external, b"different tokenizer").unwrap();
    let prior = std::fs::read(&cli_target).unwrap();
    let r = std::process::Command::new(env!("CARGO_BIN_EXE_palw-tir-frontend"))
        .arg(&path)
        .arg("--frontend-pack")
        .arg(&frontend)
        .arg("--out")
        .arg(&cli_target)
        .arg("--record")
        .arg(dir.0.join("different.json"))
        .arg("--expect-record")
        .arg(&receipt)
        .arg("--tokenizer")
        .arg(&external)
        .output()
        .unwrap();
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("FRONTEND_BUILD_MISMATCH"));
    assert_eq!(std::fs::read(&cli_target).unwrap(), prior);
    let (_, params) = artifact::read(&target, &p).unwrap();
    common::three_ways(&p, &params, &[vec![0, 0]]).unwrap();
    common::court_coverage(&p, &params, &[0, 0], &[0, 1], &[4]).unwrap();
    let mut changed_meta = metadata;
    changed_meta[0].2.last_mut().map(|b| *b ^= 1);
    std::fs::write(&path, native_gguf(&tensors, &changed_meta, None)).unwrap();
    let before = std::fs::read(&target).unwrap();
    let e = compiled.write(&target, &source, tokenizer, 8).err().unwrap().to_string();
    assert!(e.contains("header changed"), "{e}");
    assert_eq!(std::fs::read(&target).unwrap(), before);
    std::fs::write(&path, &raw).unwrap();
    let mut missing = d.clone();
    missing["quant_formats"]["published"].as_object_mut().unwrap().remove("ids");
    assert!(
        FrontendSource::open(&path, &FrontendPack::parse(&missing.to_string()).unwrap())
            .err()
            .unwrap()
            .to_string()
            .contains("needs a block descriptor")
    );
    std::fs::write(dir.0.join("second.gguf"), &raw).unwrap();
    assert!(FrontendSource::open(&dir.0, &pack).err().unwrap().to_string().contains("AMBIGUOUS"));
    assert!(FrontendSource::open(&path, &pack).is_ok());
    let conflict = std::process::Command::new(env!("CARGO_BIN_EXE_palw-tir-frontend"))
        .arg(&path)
        .arg("--frontend-pack")
        .arg(&frontend)
        .arg("--out")
        .arg(&path)
        .arg("--record")
        .arg(dir.0.join("conflict.json"))
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("FRONTEND_OUTPUT_CONFLICT"));
    assert_eq!(std::fs::read(&path).unwrap(), raw);
}

#[test]
fn native_gguf_header_bounds_reject_overflow_overlap_duplicates_and_truncation() {
    use frontend_pack::FrontendSource;
    use misaka_palw_tir_lower::{gguf::GgufFile, quantfmt::QuantRegistry};
    let dir = Temp::new();
    let (mut d, _, _, _) = saved_fixture(&stranger_gguf_descriptor(), 0);
    d["inert"] = json!(["general.architecture"]);
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let path = dir.0.join("bad.gguf");
    let floats = vec![("float".into(), vec![4], 0, vec![0; 16])];
    let reject = |raw: Vec<u8>, want: &str| {
        std::fs::write(&path, raw).unwrap();
        let e = FrontendSource::open(&path, &pack).err().unwrap().to_string();
        assert!(e.contains(want), "expected {want}: {e}");
    };
    reject(native_gguf(&[("bad".into(), vec![0], 0, vec![])], &[], None), "zero or unsupported dimension");
    reject(native_gguf(&[("bad".into(), vec![usize::MAX, 2], 0, vec![])], &[], None), "size overflow");
    reject(native_gguf(&[("bad".into(), vec![usize::MAX / 4 + 1], 0, vec![])], &[], None), "byte size overflow");
    reject(native_gguf(&floats, &[], Some(&[1])), "unaligned");
    reject(native_gguf(&floats, &[], Some(&[u64::MAX - 31])), "offset overflow");
    reject(native_gguf(&floats, &[], Some(&[32])), "past the end");
    let two = vec![floats[0].clone(), ("other".into(), vec![4], 0, vec![0; 16])];
    reject(native_gguf(&two, &[], Some(&[0, 0])), "overlapping");
    reject(native_gguf(&[("long".into(), vec![16], 0, vec![0; 64]), two[1].clone()], &[], Some(&[0, 32])), "overlapping");
    reject(native_gguf(&[floats[0].clone(), floats[0].clone()], &[], None), "twice");
    reject(native_gguf(&floats, &[gguf_meta()[0].clone(), gguf_meta()[0].clone()], None), "twice");
    reject(native_gguf(&floats, &[("general.alignment".into(), 10, (1u64 << 63).to_le_bytes().to_vec())], None), "past the end");
    reject(native_gguf(&floats, &[("flag".into(), 7, vec![2])], None), "invalid boolean");
    let mut oversized = native_gguf(&[], &[], None);
    oversized[8..16].copy_from_slice(&65_537u64.to_le_bytes());
    reject(oversized, "65537 tensors");
    let mut array = Vec::new();
    array.extend(4u32.to_le_bytes());
    array.extend((1u64 << 26).to_le_bytes());
    reject(native_gguf(&[], &[("huge".into(), 9, array)], None), "allocation limit");
    let good = native_gguf(&floats, &gguf_meta(), None);
    std::fs::write(&path, &good).unwrap();
    assert!(
        GgufFile::open_bounded(&path, QuantRegistry::builtin(), 32, 1 << 20, 10)
            .unwrap_err()
            .to_string()
            .contains("header byte limit")
    );
    assert!(
        GgufFile::open_bounded(&path, QuantRegistry::builtin(), 1 << 20, 64, 10).unwrap_err().to_string().contains("allocation limit")
    );
    reject(good[..good.len() - 1].to_vec(), "past the end");
    std::fs::write(&path, native_gguf(&floats, &[("nan".into(), 6, f32::NAN.to_le_bytes().to_vec())], None)).unwrap();
    let source = FrontendSource::open(&path, &pack).unwrap();
    assert!(source.configuration(json!({})).unwrap_err().to_string().contains("non-finite metadata"));
}

#[test]
fn native_gguf_format_ids_cannot_redefine_storage_or_bypass_descriptor_conflicts() {
    use frontend_pack::FrontendSource;
    let dir = Temp::new();
    let f = stranger_gguf_descriptor();
    let (d, s, _, _) = saved_fixture(&f, 0);
    let path = dir.0.join("model.gguf");
    std::fs::write(&path, native_gguf(&[("outside.data".into(), vec![1, 4], 65535, s.tensors["outside.data"].1.clone())], &[], None))
        .unwrap();
    for (change, want) in [
        (json!({"ids":[{"scheme":"ggml","id":2}]}), "cannot redefine"),
        (json!({"name":"F32"}), "already registered"),
        (json!({"ids":[{"scheme":"ggml","id":24}]}), "scalar integer storage cannot be redefined"),
        (json!({"ids":[{"scheme":"ggml","id":65535},{"scheme":"ggml","id":65534}]}), "exactly one GGML ID"),
    ] {
        let mut d = d.clone();
        for (k, v) in change.as_object().unwrap() {
            d["quant_formats"]["published"][k] = v.clone();
        }
        let pack = FrontendPack::parse(&d.to_string()).unwrap();
        let e = FrontendSource::open(&path, &pack).err().unwrap().to_string();
        assert!(e.contains(want), "{e}");
    }
}

#[test]
fn native_gguf_metadata_retains_signed_unsigned_and_binary32_values_exactly() {
    use frontend_pack::FrontendSource;
    let dir = Temp::new();
    let (d, _, _, _) = saved_fixture(&stranger_gguf_descriptor(), 0);
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let path = dir.0.join("metadata.gguf");
    let signed = -9_007_199_254_740_993i64;
    let unsigned = u64::MAX;
    let metadata = vec![
        ("signed".into(), 11, signed.to_le_bytes().to_vec()),
        ("unsigned".into(), 10, unsigned.to_le_bytes().to_vec()),
        ("binary32".into(), 6, 0.1f32.to_le_bytes().to_vec()),
    ];
    std::fs::write(&path, native_gguf(&[("f".into(), vec![1], 0, vec![0; 4])], &metadata, None)).unwrap();
    let source = FrontendSource::open(&path, &pack).unwrap();
    let config = source.configuration(json!({"sidecar":"public"})).unwrap();
    assert_eq!(config["signed"].as_i64(), Some(signed));
    assert_eq!(config["unsigned"].as_u64(), Some(unsigned));
    assert_eq!(config["binary32"].as_f64(), Some(0.1f32 as f64));
    assert_eq!(config["sidecar"], "public");
    assert!(source.configuration(json!({"signed":signed})).unwrap_err().to_string().contains("duplicates native key"));
}

fn split_metadata(no: u16, count: u16, total: i32) -> Vec<(String, u32, Vec<u8>)> {
    vec![
        ("split.no".into(), 2, no.to_le_bytes().to_vec()),
        ("split.count".into(), 2, count.to_le_bytes().to_vec()),
        ("split.tensors.count".into(), 5, total.to_le_bytes().to_vec()),
    ]
}

#[test]
fn split_gguf_has_identical_artifacts_receipts_and_tokenizer_identity_for_every_entry_part_and_index_order() {
    use frontend_pack::FrontendSource;
    let dir = Temp::new();
    let p = program();
    let s = source(&p);
    let mut d = definition(&p);
    d["inert"].as_array_mut().unwrap().extend([json!("general.architecture"), json!("tokenizer.ggml.tokens")]);
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let tensors: Vec<_> = s
        .tensors
        .iter()
        .map(|(name, (m, raw))| {
            let ty = match m.dtype.as_str() {
                "I8" => 24,
                "I16" => 25,
                "I64" => 27,
                _ => panic!(),
            };
            (name.clone(), m.shape.clone(), ty, raw.clone())
        })
        .collect();
    let mut metadata = gguf_meta();
    let mut tokens = Vec::new();
    tokens.extend(8u32.to_le_bytes());
    tokens.extend(16u64.to_le_bytes());
    for n in 0..16 {
        gguf_string(&mut tokens, &format!("token{n}"));
    }
    metadata.push(("tokenizer.ggml.tokens".into(), 9, tokens));
    let whole = dir.0.join("whole.gguf");
    std::fs::write(&whole, native_gguf(&tensors, &metadata, None)).unwrap();
    let native = FrontendSource::open(&whole, &pack).unwrap();
    let effective = native.configuration(config()).unwrap();
    let tokenizer = native.embedded_tokenizer_id().unwrap().unwrap();
    let compiled = pack.compile_bounded(&effective, &native, &admission::default_inputs(), 8).unwrap();
    let out = dir.0.join("whole.palwtir");
    let expected = compiled.write(&out, &native, tokenizer, 8).unwrap();
    let model = dir.0.join("split");
    std::fs::create_dir_all(&model).unwrap();
    let mut parts = Vec::new();
    for no in 0..3 {
        let path = model.join(format!("arbitrary-{:05}-of-00003.gguf", no + 1));
        let mut meta = split_metadata(no, 3, tensors.len() as i32);
        if no == 0 {
            meta.extend(metadata.clone());
        }
        let rows = match no {
            0 => &tensors[0..0],
            1 => &tensors[..2],
            _ => &tensors[2..],
        };
        std::fs::write(&path, native_gguf(rows, &meta, None)).unwrap();
        parts.push(path);
    }
    std::fs::write(model.join("config.json"), config().to_string()).unwrap();
    for (j, input) in std::iter::once(&model).chain(parts.iter()).enumerate() {
        let source = FrontendSource::open(input, &pack).unwrap();
        assert_eq!(source.configuration(config()).unwrap(), effective);
        assert_eq!(source.embedded_tokenizer_id().unwrap(), Some(tokenizer));
        assert_eq!(source.files().len(), 3);
        let built = pack.compile_bounded(&effective, &source, &admission::default_inputs(), 127).unwrap();
        let output = dir.0.join(format!("split-{j}"));
        let actual = built.write_checked(&output, &source, tokenizer, 127, Some(&expected.record)).unwrap();
        assert_eq!(actual.record, expected.record);
        assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&output).unwrap());
        let (_, params) = artifact::read(&output, &p).unwrap();
        common::three_ways(&p, &params, &[vec![1, 2, 3, 4]]).unwrap();
        common::court_coverage(&p, &params, &[1, 2, 3, 4], &[0, 1, 3], &[4]).unwrap();
        assert!(source.configuration(json!({"split.no":0})).unwrap_err().to_string().contains("inject transport"));
    }
    let indexed = dir.0.join("indexed");
    std::fs::create_dir_all(&indexed).unwrap();
    let names = ["metadata.bin", "arbitrary-A.gguf", "第三者.bin"];
    for (part, name) in parts.iter().zip(names) {
        std::fs::copy(part, indexed.join(name)).unwrap();
    }
    std::fs::copy(model.join("config.json"), indexed.join("config.json")).unwrap();
    let index = indexed.join("model.gguf.index.json");
    let manifest = json!({"format":"misaka.palw.gguf-checkpoint.v1","parts":[names[2],names[0],names[1]]});
    std::fs::write(&index, manifest.to_string()).unwrap();
    let source = FrontendSource::open(&indexed, &pack).unwrap();
    assert_eq!(source.configuration(config()).unwrap(), effective);
    assert_eq!(source.files().len(), 4);
    let built = pack.compile_bounded(&effective, &source, &admission::default_inputs(), 8).unwrap();
    let output = dir.0.join("indexed.palwtir");
    built.write_checked(&output, &source, tokenizer, 8, Some(&expected.record)).unwrap();
    let frontend = dir.0.join("frontend.json");
    let receipt = dir.0.join("record.json");
    std::fs::write(&frontend, d.to_string()).unwrap();
    std::fs::write(&receipt, serde_json::to_vec(&expected.record).unwrap()).unwrap();
    let cli = dir.0.join("cli-split");
    let r = std::process::Command::new(env!("CARGO_BIN_EXE_palw-tir-frontend"))
        .arg(&indexed)
        .arg("--frontend-pack")
        .arg(&frontend)
        .arg("--out")
        .arg(&cli)
        .arg("--record")
        .arg(dir.0.join("cli-record.json"))
        .arg("--expect-record")
        .arg(&receipt)
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&cli).unwrap());
    std::fs::write(&index, json!({"format":"misaka.palw.gguf-checkpoint.v1","parts":names}).to_string()).unwrap();
    let prior = std::fs::read(&output).unwrap();
    assert!(built.write(&output, &source, tokenizer, 8).err().unwrap().to_string().contains("index changed"));
    assert_eq!(std::fs::read(&output).unwrap(), prior);
    // A merged upstream GGUF can retain split.count=0; checked transport fields confer no identity.
    let mut merged_meta = metadata;
    merged_meta.extend(split_metadata(0, 0, tensors.len() as i32));
    std::fs::write(&whole, native_gguf(&tensors, &merged_meta, None)).unwrap();
    let merged = FrontendSource::open(&whole, &pack).unwrap();
    let built = pack.compile_bounded(&effective, &merged, &admission::default_inputs(), 8).unwrap();
    built.write_checked(&dir.0.join("merged"), &merged, tokenizer, 8, Some(&expected.record)).unwrap();
}

#[test]
fn split_gguf_refuses_missing_inconsistent_duplicated_or_hidden_headers_and_unsafe_indexes() {
    use frontend_pack::FrontendSource;
    let dir = Temp::new();
    let (d, _, _, _) = saved_fixture(&stranger_gguf_descriptor(), 0);
    let pack = FrontendPack::parse(&d.to_string()).unwrap();
    let a = dir.0.join("any-00001-of-00002.gguf");
    let b = dir.0.join("any-00002-of-00002.gguf");
    let first = vec![("a".into(), vec![1], 0, vec![0; 4])];
    let second = vec![("b".into(), vec![1], 0, vec![0; 4])];
    let initial_a = native_gguf(&first, &split_metadata(0, 2, 2), None);
    let initial_b = native_gguf(&second, &split_metadata(1, 2, 2), None);
    let reset = || {
        std::fs::write(&a, &initial_a).unwrap();
        std::fs::write(&b, &initial_b).unwrap();
    };
    reset();
    assert!(FrontendSource::open(&dir.0, &pack).is_ok());
    let reject = |input: &std::path::Path, want: &str| {
        let e = FrontendSource::open(input, &pack).err().unwrap().to_string();
        assert!(e.contains(want), "{e}");
    };
    std::fs::remove_file(&b).unwrap();
    reject(&a, "00002-of-00002");
    reset();
    std::fs::write(&b, native_gguf(&second, &split_metadata(1, 3, 2), None)).unwrap();
    reject(&a, "inconsistent part declarations");
    reset();
    std::fs::write(&b, native_gguf(&second, &split_metadata(1, 2, 3), None)).unwrap();
    reject(&a, "inconsistent part declarations");
    reset();
    for (path, no, rows) in [(&a, 0, &first), (&b, 1, &second)] {
        std::fs::write(path, native_gguf(rows, &split_metadata(no, 2, 3), None)).unwrap();
    }
    reject(&a, "total tensor count mismatch");
    reset();
    std::fs::write(&b, native_gguf(&first, &split_metadata(1, 2, 2), None)).unwrap();
    reject(&a, "duplicate tensor");
    reset();
    let mut hidden = split_metadata(1, 2, 2);
    hidden.extend(gguf_meta());
    std::fs::write(&b, native_gguf(&second, &hidden, None)).unwrap();
    reject(&a, "additional metadata");
    reset();
    let mut partial = split_metadata(0, 2, 2);
    partial.pop();
    std::fs::write(&a, native_gguf(&first, &partial, None)).unwrap();
    reject(&a, "incomplete split metadata");
    reset();
    std::fs::write(&b, native_gguf(&second, &split_metadata(0, 2, 2), None)).unwrap();
    reject(&b, "filename disagrees");
    reset();
    let mut v2 = initial_b.clone();
    v2[4..8].copy_from_slice(&2u32.to_le_bytes());
    std::fs::write(&b, v2).unwrap();
    reject(&a, "inconsistent container versions");
    reset();
    std::fs::write(&a, native_gguf(&first, &split_metadata(0, 1025, 2), None)).unwrap();
    reject(&a, "bound");
    reset();
    let index = dir.0.join("arbitrary.gguf.index.json");
    let valid = json!({"format":"misaka.palw.gguf-checkpoint.v1","parts":[a.file_name().unwrap().to_str().unwrap(),b.file_name().unwrap().to_str().unwrap()]});
    std::fs::write(&index, valid.to_string()).unwrap();
    std::fs::write(&b, native_gguf(&second, &split_metadata(0, 2, 2), None)).unwrap();
    reject(&index, "duplicate part number");
    reset();
    for names in
        [json!([]), json!(["../outside.gguf"]), json!(["/outside.gguf"]), json!(["C:\\outside.gguf"]), json!(["a.gguf", "a.gguf"])]
    {
        std::fs::write(&index, json!({"format":"misaka.palw.gguf-checkpoint.v1","parts":names}).to_string()).unwrap();
        reject(&index, "FRONTEND_GGUF_INDEX");
    }
    let mut unknown = valid.clone();
    unknown["ignored"] = json!(true);
    std::fs::write(&index, unknown.to_string()).unwrap();
    reject(&index, "unknown field");
    std::fs::write(&index, vec![b' '; frontend_pack::MAX_PACK_BYTES + 1]).unwrap();
    reject(&index, "index bytes");
}

#[test]
fn gguf_parts_share_header_allocation_and_count_budgets_before_reading_payloads() {
    use misaka_palw_tir_lower::{
        gguf::{GgufFile, GgufReadBudget},
        quantfmt::QuantRegistry,
    };
    let dir = Temp::new();
    let a = dir.0.join("a.gguf");
    let b = dir.0.join("b.gguf");
    let raw = native_gguf(&[], &split_metadata(0, 2, 0), None);
    std::fs::write(&a, &raw).unwrap();
    std::fs::write(&b, &raw).unwrap();
    let reg = QuantRegistry::builtin();
    for (header, allocation, count, want) in [
        (raw.len() as u64, 1 << 20, 100, "header byte limit"),
        (1 << 20, 1500, 100, "allocation limit"),
        (1 << 20, 1 << 20, 3, "metadata keys"),
    ] {
        let mut budget = GgufReadBudget::new(header, allocation, count);
        GgufFile::open_with_budget(&a, reg, &mut budget).unwrap();
        let e = GgufFile::open_with_budget(&b, reg, &mut budget).unwrap_err().to_string();
        assert!(e.contains(want), "{e}");
    }
}
