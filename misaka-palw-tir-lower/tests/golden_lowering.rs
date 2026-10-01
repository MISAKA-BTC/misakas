//! **The frontend regression gate** (lane G, RFC-0002 generic frontend): every checkpoint that
//! lowers today must lower to the SAME bytes whichever frontend read its config.
//!
//! For each fixture this records the BLAKE2b-512 of the lowered program's canonical encoding and the
//! file digest of the `PALWTIR1` artifact built from it with a fixed calibration (the same recipe as
//! `tests/three_way.rs`: 4 × 24 random tokens, seed 11, `QuantPolicy::default()`, an empty
//! provenance record and a zero tokenizer id). The artifact digest covers the program and every
//! integer tensor in inventory order, so it fixes the inventory root and the class id with them:
//! a refactor that changes a byte of either moves a digest here.
//!
//! The expected values live in `tests/golden/lowering_v1.json`. They were recorded on `f125bf142`
//! (`tir/lower`'s last commit before the generic frontend), when every table came from the platform's
//! libm, and must never be re-recorded to make a refactor pass: a family whose bytes change on purpose
//! is listed in `INTENDED` below with the commit that explains it. `PALW_GOLDEN_UPDATE=1` rewrites the
//! file (for adding a NEW fixture; the rewrite refuses to change an existing row that is not in
//! `INTENDED`).
//!
//! **Two math modes** (lane F's `detmath`): the baseline above is `math: "std"` (the legacy mode, held
//! byte-identical here); `tests/golden/lowering_v1_libm.json` is the same table under `math: "libm-v1"`,
//! the mode every new conversion runs in, recorded when it was introduced (`d2aecfe77`). The two differ
//! only where a table entry is a transcendental on which Apple's libm and the pure-Rust port disagree
//! by an ulp (the position temperature of Llama-4 and Ministral-3).
//!
//! **Two lowering versions** ([`misaka_palw_tir_lower::lower::LOWERING_VERSION`]): version 2
//! (`LOGITS_Q24_V1`) moves every text artifact on purpose — the head's last narrowing lands on `2^−24`
//! instead of a calibrated scale — and nothing else. So the `v1` files are held for what a deliberate
//! change must NOT move, the *program* (its digest, node count and param count), and are no longer
//! compared on the artifact digest; `lowering_v2.json` / `lowering_v2_libm.json` pin the artifacts of
//! version 2 from the commit that introduced it.

use misaka_palw_tir_lower::detmath::{MathMode, set_mode};
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::{artifact, fidelity};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Rows whose bytes changed on purpose: `name → why`.
const INTENDED: &[(&str, &str)] = &[(
    "real/deepseek-v3-fp8.json",
    "1db3c430f (lane F): FP8 checkpoints with block scales are quant-format descriptors now, so the config lowers (it was refused)",
)];

/// The two modes of `detmath`; per mode the version-1 baseline (programs only) and the version-2 one.
const MODES: &[(MathMode, &str, &str)] =
    &[(MathMode::Std, "lowering_v1.json", "lowering_v2.json"), (MathMode::LibmV1, "lowering_v1_libm.json", "lowering_v2_libm.json")];

fn golden_path(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(file)
}

fn load_golden(file: &str) -> BTreeMap<String, Value> {
    match std::fs::read(golden_path(file)) {
        Ok(b) => serde_json::from_slice(&b).expect("golden json"),
        Err(_) => BTreeMap::new(),
    }
}

/// `(program digest, artifact digest, nodes, params)` of one checkpoint directory.
fn digests(dir: &Path) -> Result<Value, String> {
    let (prep, ck) = fidelity::open_model(dir, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, ck.as_ref()).map_err(|e| e.to_string())?;
    let loader = Resident(Arc::new(params));
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24.min(max_len), 11);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet)
        .map_err(|e| format!("materialise: {e}"))?;
    let tmp = std::env::temp_dir().join(format!("palw-golden-{}-{}.palwtir", dir.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
    let file = artifact::write(&tmp, &prep.lowered.program, &mat.params, [0u8; 64], json!({})).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    let p = &prep.lowered.program;
    Ok(json!({
        "program": artifact::program_digest(p),
        "artifact": file,
        "nodes": p.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(),
        "params": p.params.len(),
    }))
}

/// The program digest alone, from a config (no weights): a real checkpoint's lowering, or its
/// refusal.
fn config_digest(text: &str) -> Value {
    match fidelity::prepare(text, &LowerOpts::default()) {
        Ok(prep) => {
            let p = &prep.lowered.program;
            json!({ "program": artifact::program_digest(p), "nodes": p.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(), "params": p.params.len() })
        }
        Err(e) => json!({ "refused": matches!(e, misaka_palw_tir_lower::LowerError::NotLowerable(_)) }),
    }
}

fn dirs(root: &str) -> Vec<(String, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(root);
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_dir() && (p.join("config.json").exists() || p.join("model.gguf").exists()))
        .map(|p| (format!("{}/{}", root.rsplit('/').next().unwrap_or(root), p.file_name().unwrap_or_default().to_string_lossy()), p))
        .collect();
    v.sort();
    v
}

fn real_configs() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real");
    let mut v: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("configs")
        .map(|e| e.expect("entry").path())
        .map(|p| (format!("real/{}", p.file_name().unwrap_or_default().to_string_lossy()), std::fs::read_to_string(&p).expect("config")))
        .collect();
    v.sort();
    v
}

#[test]
fn every_lowering_is_byte_identical_to_the_recorded_baseline() {
    // The math mode is process-wide: both modes run in this one test, one after the other.
    for (mode, v1, v2) in MODES {
        set_mode(*mode);
        check_baseline(mode.name(), v1, v2);
    }
    set_mode(MathMode::LibmV1);
}

/// The fields of a row that survive a version change that moves only artifacts: everything but the
/// artifact digest.
fn program_fields(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("artifact");
    }
    v
}

fn check_baseline(mode: &str, v1_file: &str, v2_file: &str) {
    let update = std::env::var_os("PALW_GOLDEN_UPDATE").is_some();
    let v1 = load_golden(v1_file);
    let mut v2 = load_golden(v2_file);
    let mut got: BTreeMap<String, Value> = BTreeMap::new();
    let mut failed = Vec::new();
    for root in ["tests/fixtures/hf", "tests/fixtures/hf-quant", "tests/fixtures/gguf"] {
        for (name, dir) in dirs(root) {
            // A fixture without weights (config only) is not part of the artifact baseline.
            if dir.join("config.json").exists() && !dir.join("model.safetensors").exists() && !dir.join("model.safetensors.index.json").exists() {
                continue;
            }
            match digests(&dir) {
                Ok(v) => {
                    eprintln!("[{mode}] {name:>34}: {}", v["program"].as_str().unwrap_or("").chars().take(16).collect::<String>());
                    got.insert(name, v);
                }
                Err(e) => {
                    eprintln!("[{mode}] {name:>34}: {e}");
                    failed.push(format!("{name}: {e}"));
                }
            }
        }
    }
    for (name, text) in real_configs() {
        got.insert(name, config_digest(&text));
    }
    assert!(failed.is_empty(), "{failed:?}");
    if update {
        for (k, v) in &got {
            match v2.get(k) {
                Some(old) if old != v && !INTENDED.iter().any(|(n, _)| n == k) => {
                    panic!("[{mode}] {k}: the baseline row would change ({old} → {v}); list it in INTENDED with the commit that explains it")
                }
                _ => {}
            }
            v2.insert(k.clone(), v.clone());
        }
        std::fs::create_dir_all(golden_path(v2_file).parent().expect("dir")).expect("mkdir");
        std::fs::write(golden_path(v2_file), serde_json::to_string_pretty(&v2).expect("json") + "\n").expect("write golden");
        eprintln!("[{mode}] wrote {} rows to {}", v2.len(), golden_path(v2_file).display());
    }
    // Version 1: the program of every recorded row is unchanged (its artifact moved on purpose).
    assert!(!v1.is_empty(), "[{mode}] no recorded baseline at {}", golden_path(v1_file).display());
    let mut moved = Vec::new();
    let mut missing = Vec::new();
    for (k, want) in &v1 {
        match got.get(k) {
            Some(have) if program_fields(have) == program_fields(want) => {}
            Some(have) => {
                if INTENDED.iter().any(|(n, _)| n == k) {
                    eprintln!("[{mode}] {k}: changed on purpose");
                } else {
                    moved.push(format!("{k}: {want} → {have}"));
                }
            }
            None => missing.push(k.clone()),
        }
    }
    eprintln!("[{mode}] {} rows compared against version 1 (programs), {} on purpose", v1.len(), INTENDED.len());
    assert!(moved.is_empty(), "[{mode}] lowered programs changed:\n{}", moved.join("\n"));
    assert!(missing.is_empty(), "[{mode}] baseline rows no longer produced: {missing:?}");
    // Version 2: every byte.
    if update {
        return;
    }
    assert!(!v2.is_empty(), "[{mode}] no version-2 baseline at {} (record it with PALW_GOLDEN_UPDATE=1)", golden_path(v2_file).display());
    let mut moved = Vec::new();
    for (k, want) in &v2 {
        match got.get(k) {
            Some(have) if have == want => {}
            Some(have) => moved.push(format!("{k}: {want} → {have}")),
            None => moved.push(format!("{k}: no longer produced")),
        }
    }
    assert!(moved.is_empty(), "[{mode}] lowered bytes (version 2) changed:\n{}", moved.join("\n"));
}

/// The audit directories of the 09-29 sweep (`~/Downloads/MISAKA-wt-b/tir-audit/<family>/`, or
/// `$PALW_TIR_AUDIT_DIR`) hold the `PALWTIR1` containers the SDK declared classes from. Where the
/// directory exists, the program inside each container must be the one the config lowers to now:
/// an oracle independent of this file's own baseline. A family whose program moved since the audit
/// for a reason that is not the frontend is listed in `AUDIT_MOVED`: `falcon_alibi`'s audit run lowered
/// it with `--max-window` (its bfloat16 ALiBi table has one row per position the program serves), the
/// default options here do not.
const AUDIT_MOVED: &[&str] = &["falcon_alibi"];

#[test]
fn the_audit_containers_hold_the_programs_the_configs_lower_to() {
    let root = std::env::var_os("PALW_TIR_AUDIT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Downloads/MISAKA-wt-b/tir-audit"));
    let Ok(rd) = std::fs::read_dir(&root) else {
        eprintln!("SKIPPED: no audit directory at {}", root.display());
        return;
    };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join("lowered.palwtir").exists() && p.join("config.json").exists()).collect();
    dirs.sort();
    let (mut equal, mut moved) = (0, Vec::new());
    for d in &dirs {
        let name = d.file_name().unwrap_or_default().to_string_lossy().to_string();
        let cfg = std::fs::read_to_string(d.join("config.json")).expect("config");
        let Ok(prep) = fidelity::prepare(&cfg, &LowerOpts::default()) else { continue };
        let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(&d.join("lowered.palwtir")).expect("container");
        if c.program == prep.lowered.program {
            equal += 1;
        } else if !AUDIT_MOVED.contains(&name.as_str()) {
            moved.push(name);
        }
    }
    eprintln!("{equal} audit containers hold today's program; moved: {moved:?}");
    assert!(moved.is_empty(), "audit containers whose program differs from today's lowering: {moved:?}");
}
