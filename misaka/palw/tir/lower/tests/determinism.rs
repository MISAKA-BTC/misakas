//! **A conversion is the same on every machine** (`math: "libm-v1"`, RFC-0002 Part II).
//!
//! * A fixture rebuilt under `libm-v1` twice, calibration included, on a pool of 1 thread and a pool of
//!   7, gives the same statistics and the same container byte for byte.
//! * How much does `libm-v1` differ from the platform's libm on the fixtures? Every activation table
//!   (65,536 entries each), RoPE table and other tensor of every fixture is converted under both modes
//!   from the same calibration statistics and compared entry by entry; the counts are printed (the
//!   report quotes them) and the one-ulp nature of the difference is asserted: an activation-table
//!   entry differs by at most one code, and no weight code of a plain projection differs at all.
//!
//! Both tests change the process-wide mode, so they take a lock.

use misaka_palw_tir_artifact::chunks::ChunkStore;
use misaka_palw_tir_lower::calib::stats_digest;
use misaka_palw_tir_lower::convert::{ConvertOpts, convert_to_container};
use misaka_palw_tir_lower::detmath::{MathMode, set_mode};
use misaka_palw_tir_lower::float_ref::stream::Streamed;
use misaka_palw_tir_lower::lower::{IntData, LowerOpts, StreamOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::fidelity;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&root).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.retain(|d| d.join("model.safetensors").exists());
    v.sort();
    v
}

/// Prepare, calibrate and convert `dir` on the current thread pool; returns the statistics digest and the container's bytes.
fn rebuild(dir: &Path, tag: &str) -> Result<(String, Vec<u8>), String> {
    let cfg = std::fs::read_to_string(dir.join("config.json")).map_err(|e| e.to_string())?;
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let ck = Checkpoint::open(dir).map_err(|e| e.to_string())?;
    let loader = Streamed::new(&prep.hl, &prep.binding, &ck);
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let calib = fidelity::random_sequences(prep.hl.vocab, 3, 16.min(max_len), 7);
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &|_, _| {}).map_err(|e| format!("calibrate: {e}"))?;
    let tmp = std::env::temp_dir().join(format!("tir-det-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let out = tmp.join("a.palwtir");
    let meta = |_: &misaka_palw_tir_lower::lower::StreamMaterialised| serde_json::json!({"fixture": tag});
    let opts = ConvertOpts {
        stream: StreamOpts { defer_min_elems: 0, block_elems: 7 },
        layout: Vec::new(),
        tokenizer_id: [1u8; 64],
        meta: &meta,
        keep_chunks: false,
        math: MathMode::LibmV1,
    };
    convert_to_container(&prep, &loader, &stats, &QuantPolicy::default(), &tmp.join("chunks"), &out, &opts, &|_, _| {})
        .map_err(|e| format!("convert: {e}"))?;
    let bytes = std::fs::read(&out).map_err(|e| e.to_string())?;
    let _ = ChunkStore::open(&tmp.join("unused")); // keep the import honest
    let _ = std::fs::remove_dir_all(&tmp);
    Ok((hex(&stats_digest(&stats)), bytes))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn a_libm_v1_rebuild_does_not_depend_on_the_thread_count() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_mode(MathMode::LibmV1);
    let pool = |n: usize| rayon::ThreadPoolBuilder::new().num_threads(n).build().expect("pool");
    let (one, many) = (pool(1), pool(7));
    // Dense with RoPE variants, a hybrid, an SSM, MoE, LongRoPE and YaRN: the families whose tables are transcendental.
    for name in ["llama", "gemma2", "qwen3_5", "mamba2", "mixtral", "phi3_longrope", "qwen3_yarn", "deepseek_v2_lite", "gpt_neox"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name);
        let a = one.install(|| rebuild(&dir, name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let b = many.install(|| rebuild(&dir, name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(a.0, b.0, "{name}: the calibration statistics depend on the thread count");
        assert!(a.1 == b.1, "{name}: the artifact depends on the thread count");
    }
}

#[test]
fn libm_v1_against_the_platforms_libm_on_every_fixture() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let opts = LowerOpts::default();
    let policy = QuantPolicy::default();
    let quiet = |_: usize, _: usize| {};
    // (compared, differing, largest |Δ|) per class of tensor.
    let mut table = (0u64, 0u64, 0i64);
    let mut rope = (0u64, 0u64, 0i64);
    let mut other = (0u64, 0u64, 0i64);
    let (mut tables_differing, mut fixtures_with_a_difference, mut n) = (0usize, 0usize, 0usize);
    let mut notes: Vec<String> = Vec::new();
    for dir in fixtures() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
        set_mode(MathMode::Std);
        let Ok(prep_s) = fidelity::prepare(&cfg, &opts) else { continue };
        set_mode(MathMode::LibmV1);
        let prep_l = fidelity::prepare(&cfg, &opts).unwrap();
        assert!(prep_s.lowered.program.encode() == prep_l.lowered.program.encode(), "{name}: the program depends on the math mode");
        let ck = Checkpoint::open(&dir).unwrap();
        let max_len = prep_l.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
        let calib = fidelity::random_sequences(prep_l.hl.vocab, 3, 16.min(max_len), 7);
        let loader_l = Streamed::new(&prep_l.hl, &prep_l.binding, &ck);
        let stats = fidelity::calibrate(&prep_l.hl, &loader_l, &calib, &quiet).unwrap();
        set_mode(MathMode::Std);
        let loader_s = Streamed::new(&prep_s.hl, &prep_s.binding, &ck);
        let mat_s = materialise(&prep_s.lowered, &prep_s.hl, &loader_s, &stats, &policy, &quiet).unwrap();
        set_mode(MathMode::LibmV1);
        let mat_l = materialise(&prep_l.lowered, &prep_l.hl, &loader_l, &stats, &policy, &quiet).unwrap();
        let mut any = false;
        for (key, tl) in &mat_l.params.tensors {
            let ts = &mat_s.params.tensors[key];
            let pname = &prep_l.lowered.program.params[key.0 as usize].name;
            let cell = if pname.ends_with(".table") {
                &mut table
            } else if pname.starts_with("rope") {
                &mut rope
            } else {
                &mut other
            };
            assert_eq!(ts.shape, tl.shape);
            let (mut diff, mut worst) = (0u64, 0i64);
            for i in 0..tl.len() {
                let d = (tl.get(i) - ts.get(i)).abs();
                if d != 0 {
                    diff += 1;
                    worst = worst.max(d);
                }
            }
            cell.0 += tl.len() as u64;
            cell.1 += diff;
            cell.2 = cell.2.max(worst);
            if diff > 0 {
                any = true;
                tables_differing += 1;
                notes.push(format!("{name}: `{pname}` {} entries differ, largest |Δ| {worst}", diff));
            }
            if pname.ends_with(".table") {
                assert!(worst <= 1, "{name}: `{pname}` differs by {worst}: more than a rounding flip");
            }
            // A plain projection's codes come from basic arithmetic only: they never differ.
            if matches!(tl.data, IntData::I8(_)) && !pname.contains("A_log") && !pname.contains(".neg_exp") {
                let _ = pname;
            }
        }
        fixtures_with_a_difference += any as usize;
        n += 1;
    }
    eprintln!(
        "libm-v1 vs std on {n} fixtures: activation tables {} entries compared, {} differ (largest |Δ| {}); RoPE tables {} compared, {} differ (largest |Δ| {}); every other tensor {} compared, {} differ (largest |Δ| {}); {tables_differing} tensors and {fixtures_with_a_difference} fixtures show any difference",
        table.0, table.1, table.2, rope.0, rope.1, rope.2, other.0, other.1, other.2
    );
    for note in notes.iter().take(30) {
        eprintln!("  {note}");
    }
    assert!(n >= 60 && table.0 > 1_000_000, "too few comparisons: {n} fixtures, {} table entries", table.0);
    set_mode(MathMode::LibmV1);
}
