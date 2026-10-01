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
//! (`tir/lower`'s last commit before the generic frontend) and must never be re-recorded to make a
//! refactor pass: a family whose bytes change on purpose is listed in `INTENDED` below with the
//! commit that explains it. `PALW_GOLDEN_UPDATE=1` rewrites the file (for adding a NEW fixture; the
//! rewrite refuses to change an existing row that is not in `INTENDED`).

use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::{artifact, fidelity};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Rows whose bytes changed on purpose: `name → why`. Empty at the baseline.
const INTENDED: &[(&str, &str)] = &[
    // 537ca553f, math "libm-v1": the conversion's transcendentals come from a pinned pure-Rust libm, not the platform's. The one table
    // they move on these fixtures is the Q24 query-temperature table `attn.q_temp.t` (a binary32 `ln_1p` ulp, 775 entries by up to 8
    // codes); every program digest is unchanged, and so is every other row.
    ("hf/llama4", "libm-v1: the query-temperature table's binary32 ln_1p (537ca553f)"),
    ("hf/llama4_vlm", "libm-v1: the query-temperature table's binary32 ln_1p (537ca553f)"),
    ("hf/ministral3", "libm-v1: the query-temperature table's binary32 ln_1p (537ca553f)"),
    // 1db3c430f: FP8 with block scales is a built-in quant-format descriptor, so a config that was refused for being quantised lowers.
    ("real/deepseek-v3-fp8.json", "FP8_BLOCK descriptor: refused before, lowers now (1db3c430f)"),
];

fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/lowering_v1.json")
}

fn load_golden() -> BTreeMap<String, Value> {
    match std::fs::read(golden_path()) {
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
    let update = std::env::var_os("PALW_GOLDEN_UPDATE").is_some();
    let mut golden = load_golden();
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
                    eprintln!("{name:>34}: {}", v["program"].as_str().unwrap_or("").chars().take(16).collect::<String>());
                    got.insert(name, v);
                }
                Err(e) => {
                    eprintln!("{name:>34}: {e}");
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
            match golden.get(k) {
                Some(old) if old != v && !INTENDED.iter().any(|(n, _)| n == k) => {
                    panic!("{k}: the baseline row would change ({old} → {v}); list it in INTENDED with the commit that explains it")
                }
                _ => {}
            }
            golden.insert(k.clone(), v.clone());
        }
        std::fs::create_dir_all(golden_path().parent().expect("dir")).expect("mkdir");
        std::fs::write(golden_path(), serde_json::to_string_pretty(&golden).expect("json") + "\n").expect("write golden");
        eprintln!("wrote {} rows to {}", golden.len(), golden_path().display());
        return;
    }
    assert!(!golden.is_empty(), "no recorded baseline at {}", golden_path().display());
    let mut moved = Vec::new();
    let mut missing = Vec::new();
    for (k, want) in &golden {
        match got.get(k) {
            Some(have) if have == want => {}
            Some(have) => {
                if INTENDED.iter().any(|(n, _)| n == k) {
                    eprintln!("{k}: changed on purpose");
                } else {
                    moved.push(format!("{k}: {want} → {have}"));
                }
            }
            None => missing.push(k.clone()),
        }
    }
    eprintln!("{} rows compared, {} on purpose", golden.len(), INTENDED.len());
    assert!(moved.is_empty(), "lowered bytes changed:\n{}", moved.join("\n"));
    assert!(missing.is_empty(), "baseline rows no longer produced: {missing:?}");
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
