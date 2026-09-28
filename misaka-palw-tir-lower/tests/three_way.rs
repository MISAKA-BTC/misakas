//! **Freeze criterion 4 on the HF-lowered programs**: for every one of the 61 tiny-fixture
//! architectures (and the 11 pre-quantised GPTQ/AWQ fixtures), the lowered program with its
//! calibrated integer params is run on
//!
//! * the reference evaluator (`misaka-palw-tir`),
//! * the independent second implementation (`misaka-palw-tir-ref2`, written from 04b alone, fed
//!   the canonical bytes — it decodes them with its own codec), and
//! * the typed backend that ships on nodes (`misaka-palw-tir-exec`),
//!
//! and at every position the logits and every commit point (slot, block, layer, node, value)
//! must be equal across all three, byte for byte.

use misaka_palw_tir::Interpreter;
use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{IntParams, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use std::path::Path;
use std::sync::Arc;

/// One commit: `(slot, block, layer, node, values)`.
type Commit = (u64, u8, Option<u32>, u16, Vec<i128>);

struct Collect(Vec<Commit>);
impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.push((v.slot as u64, v.block, v.layer.map(u32::from), v.node, v.data.to_i128s()));
        }
    }
}

fn ref2_dtype(d: misaka_palw_tir::DType) -> misaka_palw_tir_ref2::DType {
    use misaka_palw_tir::DType as A;
    use misaka_palw_tir_ref2::DType as B;
    match d {
        A::I8 => B::I8,
        A::I16 => B::I16,
        A::I32 => B::I32,
        A::I64 => B::I64,
        A::I128 => B::I128,
        A::Idx => B::Idx,
    }
}

/// Positions run on all three for one fixture, or why it could not be prepared.
fn run(name: &str) -> Result<usize, String> {
    run_in("tests/fixtures/hf", name)
}

fn run_in(root: &str, name: &str) -> Result<usize, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(root).join(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).map_err(|e| e.to_string())?;
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let ck = Checkpoint::open(&dir).map_err(|e| e.to_string())?;
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(|e| e.to_string())?;
    let loader = Resident(Arc::new(params));
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24.min(max_len), 11);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet)
        .map_err(|e| format!("materialise: {e}"))?;
    let eval = fidelity::random_sequences(prep.hl.vocab, 2, 16.min(max_len), 97);
    three_ways(&prep.lowered.program, &mat.params, &eval)
}

/// Run `p` with `params` over `eval` on the reference evaluator, ref2 and the typed backend; every
/// position's logits and commits must be equal. Returns the positions run.
fn three_ways(p: &misaka_palw_tir::TirProgramV1, params: &IntParams, eval: &[Vec<usize>]) -> Result<usize, String> {
    let IntParams { tensors } = params;

    // ref2: its own decoding of the canonical bytes, its own tensors.
    let bytes = p.encode();
    let p2 = misaka_palw_tir_ref2::codec::decode_canonical(&bytes).map_err(|e| format!("ref2 refuses the program: {e:?}"))?;
    let mut params2 = misaka_palw_tir_ref2::eval::Params::new();
    for ((j, layer), t) in tensors {
        let d = &p2.params[*j as usize];
        let shape = d.shape.iter().map(|x| *x as u64).collect();
        let t2 =
            misaka_palw_tir_ref2::Tensor::from_le_bytes(d.dtype, shape, &t.le_bytes()).map_err(|e| format!("ref2 tensor: {e:?}"))?;
        assert_eq!(d.dtype, ref2_dtype(p.params[*j as usize].dtype));
        params2.insert((*j, layer.map(u32::from)), t2);
    }
    // exec: the plan and borrowed little-endian params.
    let owned: Vec<((u16, Option<u16>), Vec<u8>)> = tensors.iter().map(|(k, t)| (*k, t.le_bytes())).collect();
    let plan = TirPlan::compile(p).map_err(|e| format!("exec plan: {e}"))?;
    let mut xparams = TirParams::new(&plan);
    for ((j, layer), b) in &owned {
        let data = ParamData::from_le_bytes(p.params[*j as usize].dtype, b).map_err(|e| e.to_string())?;
        xparams.insert(&plan, *j, *layer, data).map_err(|e| e.to_string())?;
    }

    let interp = Interpreter::new(p).map_err(|e| e.to_string())?;
    let mut positions = 0;
    for seq in eval {
        let mut st1 = misaka_palw_tir::RunState::default();
        let mut st2 = misaka_palw_tir_ref2::eval::initial_state(&p2);
        let mut exec = TirExecutor::new(&plan, &xparams).map_err(|e| e.to_string())?;
        for (pos, tok) in seq.iter().enumerate() {
            let o1 = interp.step(params, &mut st1, *tok as u32).map_err(|e| format!("reference at {pos}: {e}"))?;
            let (o2, next) =
                misaka_palw_tir_ref2::eval::step(&p2, &params2, &st2, *tok as u64).map_err(|e| format!("ref2 at {pos}: {e:?}"))?;
            st2 = next;
            let mut sink = Collect(Vec::new());
            exec.step(*tok as u32, &mut sink).map_err(|e| format!("exec at {pos}: {e}"))?;
            let (_, xl) = exec.logits();
            let l1 = &o1.logits.data;
            if *l1 != o2.logits.data || *l1 != xl.to_i128s() {
                return Err(format!("position {pos}: the logits differ"));
            }
            let c1: Vec<Commit> =
                o1.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())).collect();
            let c2: Vec<Commit> = o2.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
            let mut c3 = sink.0;
            c3.sort_by_key(|c| c.0);
            if c1 != c2 {
                return Err(format!("position {pos}: reference and ref2 commit differently ({} vs {} commits)", c1.len(), c2.len()));
            }
            if c1 != c3 {
                return Err(format!("position {pos}: reference and exec commit differently ({} vs {} commits)", c1.len(), c3.len()));
            }
            positions += 1;
        }
    }
    Ok(positions)
}

#[test]
fn every_hf_tiny_fixture_program_is_the_same_on_all_three_implementations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut names: Vec<String> =
        std::fs::read_dir(&root).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names.len(), 61);
    let mut failed = Vec::new();
    let mut total = 0;
    for n in &names {
        let has_weights = root.join(n).join("model.safetensors").exists();
        if !has_weights {
            eprintln!("{n:>22}: SKIPPED (no weights)");
            continue;
        }
        match run(n) {
            Ok(k) => {
                total += k;
                eprintln!("{n:>22}: {k} positions, logits and every commit equal on reference, ref2 and exec");
            }
            Err(e) => {
                eprintln!("{n:>22}: {e}");
                failed.push(n.clone());
            }
        }
    }
    eprintln!("{total} positions in all");
    assert!(failed.is_empty(), "{failed:?}");
}

/// The pre-quantised fixtures (`tests/quantized.rs`): the grouped integer MatMul, the per-group
/// scale products and sums, the zero-point term and the column-order gather are the same bytes on
/// all three implementations.
#[test]
fn every_prequantised_fixture_is_the_same_on_all_three_implementations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-quant");
    let mut names: Vec<String> =
        std::fs::read_dir(&root).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names.len(), 11);
    let mut failed = Vec::new();
    for n in &names {
        match run_in("tests/fixtures/hf-quant", n) {
            Ok(k) => eprintln!("{n:>22}: {k} positions, logits and every commit equal on reference, ref2 and exec"),
            Err(e) => {
                eprintln!("{n:>22}: {e}");
                failed.push(n.clone());
            }
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
}

/// The LoRA candidates (`tests/lora.rs`) on all three implementations: the adapter path's
/// `i32 × i16` and `i16 × i16` products, the exact rational scale (`Mul`, rounded `Div`) and the
/// added narrowing are the same bytes everywhere.
#[test]
fn every_lora_candidate_is_the_same_on_all_three_implementations() {
    use misaka_palw_tir_lower::weights::Overlay;
    use misaka_palw_tir_lower::{hf_weights, hl, lora, lower};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut names: Vec<String> =
        std::fs::read_dir(root.join("hf-lora")).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert!(names.len() >= 4);
    for name in names {
        let ad_dir = root.join("hf-lora").join(&name);
        let r: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
        let base_dir = root.join("hf").join(r["base"].as_str().unwrap());
        let mut spec = misaka_palw_tir_lower::parse_config_str(&std::fs::read_to_string(base_dir.join("config.json")).unwrap()).unwrap();
        lora::attach(&mut spec, &std::fs::read_to_string(ad_dir.join("adapter_config.json")).unwrap()).unwrap();
        let hlp = hl::build_program(&spec).unwrap();
        let bind = hf_weights::bind(&spec, &hlp).unwrap();
        let (ck, ad) = (Checkpoint::open(&base_dir).unwrap(), Checkpoint::open(&ad_dir.join("adapter_model.safetensors")).unwrap());
        let (params, _) = ParamStore::from_source(&hlp, &bind, &Overlay { base: &ck, over: &ad }).unwrap();
        let loader = Resident(Arc::new(params));
        let quiet = |_: usize, _: usize| {};
        let stats = fidelity::calibrate(&hlp, &loader, &fidelity::random_sequences(hlp.vocab, 4, 24, 11), &quiet).unwrap();
        let mut lw = lower::lower(&hlp, &LowerOpts { max_window: Some(64), ..LowerOpts::default() }).unwrap();
        lower::adapter_params_last(&mut lw).unwrap();
        let mat = materialise(&lw, &hlp, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
        let eval = fidelity::random_sequences(hlp.vocab, 2, 16, 97);
        match three_ways(&lw.program, &mat.params, &eval) {
            Ok(n) => eprintln!("{name}: {n} positions equal on reference, ref2 and exec"),
            Err(e) => panic!("{name}: {e}"),
        }
    }
}
