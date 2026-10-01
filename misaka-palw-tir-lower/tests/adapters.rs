//! **Adapters are data and they say what the Rust parsers said.**
//!
//! Every built-in adapter is read through the generic reader's evaluator and must produce, for each
//! fixture of its family, exactly the `ModelSpec` the per-architecture Rust parser produced (notes,
//! confidence and the reference label aside). The Rust parsers stay in the tree only as this oracle
//! until the last family is converted; the golden lowering gate (`tests/golden_lowering.rs`) is the
//! permanent one.

use misaka_palw_tir_lower::adapter::{self, builtin};
use misaka_palw_tir_lower::spec::{Confidence, ModelSpec, Reference};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn fixture_config(root: &str, name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(root).join(name).join("config.json");
    serde_json::from_str(&misaka_palw_tir_lower::hf_config::sanitize_json(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))))
        .expect("json")
}

fn real_config(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real").join(format!("{name}.json"));
    serde_json::from_str(&misaka_palw_tir_lower::hf_config::sanitize_json(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))))
        .expect("json")
}

/// The first path at which two JSON values differ.
fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (l, r) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                if let Some(d) = first_diff(l, r, &format!("{path}.{k}")) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: {} vs {} elements", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (l, r))| first_diff(l, r, &format!("{path}[{i}]")))
        }
        (Value::Number(l), Value::Number(r)) if l.as_f64() == r.as_f64() => None,
        _ if a == b => None,
        _ => Some(format!("{path}: {a} ≠ {b}")),
    }
}

/// The spec without what is informational: notes, confidence, the reference label, the family label
/// (`model_type`) and the corpus tags.
fn comparable(mut s: ModelSpec) -> Value {
    s.notes.clear();
    s.model_type.clear();
    s.families.clear();
    s.confidence = Confidence::Known;
    s.reference = Reference::Native;
    serde_json::to_value(&s).expect("json")
}

fn arch_of(cfg: &Value) -> String {
    cfg.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Every (fixture or real config, built-in adapter claiming its architecture) pair.
fn pairs() -> Vec<(String, String, Value)> {
    let mut out = Vec::new();
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(tests.join("fixtures/hf")).expect("fixtures").map(|e| e.expect("entry").path()).collect();
    dirs.sort();
    let mut cfgs: Vec<(String, Value)> = dirs
        .iter()
        .map(|d| (format!("hf/{}", d.file_name().unwrap_or_default().to_string_lossy()), fixture_config("hf", &d.file_name().unwrap_or_default().to_string_lossy())))
        .collect();
    let mut reals: Vec<PathBuf> = std::fs::read_dir(tests.join("configs/real")).expect("configs").map(|e| e.expect("entry").path()).collect();
    reals.sort();
    cfgs.extend(reals.iter().map(|p| {
        let n = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
        (format!("real/{n}"), real_config(&n))
    }));
    for (name, cfg) in cfgs {
        let arch = arch_of(&cfg);
        if let Some(a) = builtin::all().iter().find(|a| a.architectures().contains(&arch.as_str())) {
            out.push((a.id.clone(), name, cfg));
        }
    }
    out
}

/// The legacy parser's refusal or its spec, the adapter's refusal or its spec: both must agree.
fn compare_outcomes(id: &str, name: &str, cfg: &Value) {
    use misaka_palw_tir_lower::hf_schema::{AdapterChoice, ReadOptions, read_model};
    let l = misaka_palw_tir_lower::hf_config::parse_legacy(cfg);
    let r = read_model(cfg, None, &ReadOptions { adapter: AdapterChoice::BuiltIn(id.to_string()) });
    match (l, r) {
        (Ok(l), Ok(r)) => {
            if let Some(d) = first_diff(&comparable(l), &comparable(r.spec), "") {
                panic!("adapter `{id}` on {name} differs from the Rust parser at {d}");
            }
        }
        (Err(_), Err(_)) => {}
        (Ok(_), Err(e)) => panic!("adapter `{id}` refuses {name} ({e}) but the Rust parser reads it"),
        (Err(e), Ok(_)) => panic!("adapter `{id}` reads {name} but the Rust parser refuses it ({e})"),
    }
}

#[test]
fn every_builtin_adapter_says_what_the_rust_parser_said() {
    let pairs = pairs();
    let mut by: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (id, name, cfg) in &pairs {
        compare_outcomes(id, name, cfg);
        *by.entry(id.clone()).or_default() += 1;
    }
    eprintln!("{} (config, adapter) pairs agree with the Rust parsers: {by:?}", pairs.len());
}

#[test]
fn the_pack_parses_and_every_file_is_listed() {
    assert_eq!(builtin::all().len(), builtin::FILES.len(), "a built-in adapter does not parse");
    let dir: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("adapters");
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .expect("adapters dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".json"))
        .map(|n| n.trim_end_matches(".json").to_string())
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = builtin::FILES.iter().map(|(i, _)| i.to_string()).collect();
    listed.sort();
    assert_eq!(on_disk, listed, "adapters/*.json and the pack list in src/adapter/builtin.rs disagree");
    for a in builtin::all() {
        assert_eq!(a.hash.len(), 128);
    }
    eprintln!("pack hash {}", builtin::pack_hash());
}

#[test]
fn canonical_json_is_key_order_and_float_spelling_independent() {
    let a: Value = serde_json::from_str(r#"{"b": 1.0, "a": [1, {"y": 2, "x": 3}], "c": 0.5}"#).unwrap();
    let b: Value = serde_json::from_str(r#"{"c": 0.5, "a": [1, {"x": 3, "y": 2}], "b": 1}"#).unwrap();
    assert_eq!(adapter::canonical_json(&a), adapter::canonical_json(&b));
    assert_eq!(adapter::hash_value(&a), adapter::hash_value(&b));
    assert_eq!(adapter::canonical_json(&a), r#"{"a":[1,{"x":3,"y":2}],"b":1,"c":0.5}"#);
}

/// **Level A coverage**: how many of today's families a configuration reads into the same spec with
/// NO adapter at all (the Level A template, `standard-decoder`, alone), judged on the fixtures (read
/// with their tensor names, as `palw-class check-architecture <hf dir>` does) and on the real
/// configs (config alone). A family needs an adapter exactly when its class departs from the
/// standard keys' reading; this prints, per family, whether it does.
#[test]
fn level_a_coverage_of_the_families_with_an_adapter() {
    use misaka_palw_tir_lower::hf_schema::{AdapterChoice, ReadOptions, TensorIndex, read_model};
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let pairs = pairs();
    // (configs, identical, identical with tensor names) per family.
    let mut by_family: std::collections::BTreeMap<String, (usize, usize, usize, Vec<String>)> = std::collections::BTreeMap::new();
    for (id, name, cfg) in &pairs {
        let with = read_model(cfg, None, &ReadOptions { adapter: AdapterChoice::BuiltIn(id.clone()) });
        let tensors = name.strip_prefix("hf/").and_then(|n| TensorIndex::from_checkpoint_path(&tests.join("fixtures/hf").join(n)).ok());
        let e = by_family.entry(id.clone()).or_default();
        e.0 += 1;
        let Ok(w) = with else { continue };
        let want = comparable(w.spec);
        for (slot, t) in [(1usize, None), (2usize, tensors.as_ref())] {
            if slot == 2 && t.is_none() {
                continue;
            }
            match read_model(cfg, t, &ReadOptions { adapter: AdapterChoice::None }) {
                Ok(n) => match first_diff(&want, &comparable(n.spec), "") {
                    None => {
                        if slot == 1 {
                            e.1 += 1
                        } else {
                            e.2 += 1
                        }
                    }
                    Some(d) if slot == 2 => e.3.push(format!("{name}: differs at {d}")),
                    Some(_) => {}
                },
                Err(f) if slot == 2 => e.3.push(format!("{name}: {}", f.error.to_string().chars().take(100).collect::<String>())),
                Err(_) => {}
            }
        }
    }
    let (mut automatic, mut automatic_t) = (0, 0);
    for (id, (total, same, same_t, why)) in &by_family {
        let with_t = pairs.iter().filter(|(i, n, _)| i == id && n.starts_with("hf/")).count();
        let all_t = *same_t == with_t && with_t > 0;
        eprintln!(
            "{id:>14}: {same}/{total} identical from the config alone, {same_t}/{with_t} with tensor names{}{}",
            if all_t { "  ← Level A with tensors" } else { "" },
            if why.is_empty() { String::new() } else { format!("  e.g. {}", why[0]) }
        );
        if *same == *total {
            automatic += 1;
        }
        if all_t {
            automatic_t += 1;
        }
    }
    eprintln!(
        "{automatic} of {} families read identically from the config alone; {automatic_t} with the checkpoint's tensor names",
        by_family.len()
    );
}
