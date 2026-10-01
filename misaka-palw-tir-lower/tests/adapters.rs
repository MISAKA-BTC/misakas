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

/// Every (fixture or real config, built-in adapter claiming its architecture) pair, over every
/// fixture root and config directory the crate carries.
fn pairs() -> Vec<(String, String, Value)> {
    let mut out = Vec::new();
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut cfgs: Vec<(String, Value)> = Vec::new();
    // `tests/fixtures/<root>/<name>/config.json`
    for root in ["hf", "hf-enc", "hf-quant", "hf-lora", "hf-vis", "hf-encdec"] {
        let Ok(rd) = std::fs::read_dir(tests.join("fixtures").join(root)) else { continue };
        let mut dirs: Vec<PathBuf> = rd.map(|e| e.expect("entry").path()).collect();
        dirs.sort();
        for d in dirs {
            let n = d.file_name().unwrap_or_default().to_string_lossy().to_string();
            if d.join("config.json").exists() {
                cfgs.push((format!("{root}/{n}"), fixture_config(root, &n)));
            }
        }
    }
    // `tests/configs/<dir>/<name>.json`: configs of published checkpoints
    for dir in ["real", "encoders", "legacy", "tiny", "encdec"] {
        let Ok(rd) = std::fs::read_dir(tests.join("configs").join(dir)) else { continue };
        let mut files: Vec<PathBuf> = rd.map(|e| e.expect("entry").path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        files.sort();
        for p in files {
            let n = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            let v: Value = serde_json::from_str(&misaka_palw_tir_lower::hf_config::sanitize_json(&text)).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            cfgs.push((format!("{dir}/{n}"), v));
        }
    }
    for (name, cfg) in cfgs {
        let arch = arch_of(&cfg);
        if let Some(a) = builtin::all().iter().find(|a| a.architectures().contains(&arch.as_str())) {
            out.push((a.id.clone(), name, cfg));
        }
    }
    out
}

/// What the two readers said about one configuration.
enum Outcome {
    Agree { read: bool },
    /// The Rust parser panicked (an unbounded divide or index on a hostile config — its own defect);
    /// the adapter is only required not to panic.
    LegacyPanicked,
    /// The adapter refuses, through the generic validation of the produced spec or the evaluator's
    /// total arithmetic, a configuration no Hugging Face class can run (a head count that does not
    /// divide the width, an odd rotary dimension, a division by zero, a non-finite number) where
    /// the Rust parser read it without looking. Stricter, never looser.
    Stricter(String),
    Differ(String),
}

/// Messages of the generic checks an adapter's result goes through.
const STRICTER: &[&str] =
    &["heads, kv heads and head dim are inconsistent", "must be even and positive", "division by zero in an adapter expression", "produced an invalid ModelSpec", "needs at least one layer"];

/// The legacy parser's refusal or its spec, the adapter's refusal or its spec: both must agree. An
/// adapter must never panic, whatever the configuration says.
fn outcome(id: &str, name: &str, cfg: &Value) -> Outcome {
    use misaka_palw_tir_lower::hf_schema::{AdapterChoice, ReadOptions, read_model};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let l = catch_unwind(AssertUnwindSafe(|| misaka_palw_tir_lower::hf_config::parse_legacy(cfg)));
    let r = match catch_unwind(AssertUnwindSafe(|| read_model(cfg, None, &ReadOptions { adapter: AdapterChoice::BuiltIn(id.to_string()) }))) {
        Ok(r) => r,
        Err(_) => return Outcome::Differ(format!("adapter `{id}` PANICS on {name}")),
    };
    match (l, r) {
        (Ok(Ok(l)), Ok(r)) => match first_diff(&comparable(l), &comparable(r.spec), "") {
            None => Outcome::Agree { read: true },
            Some(d) => Outcome::Differ(format!("adapter `{id}` on {name} differs from the Rust parser at {d}")),
        },
        (Ok(Err(_)), Err(_)) => Outcome::Agree { read: false },
        (Ok(Ok(_)), Err(e)) if STRICTER.iter().any(|m| e.to_string().contains(m)) => Outcome::Stricter(format!("adapter `{id}` refuses {name} ({e}) but the Rust parser reads it")),
        (Ok(Ok(_)), Err(e)) => Outcome::Differ(format!("adapter `{id}` refuses {name} ({e}) but the Rust parser reads it")),
        (Ok(Err(e)), Ok(_)) => Outcome::Differ(format!("adapter `{id}` reads {name} but the Rust parser refuses it ({e})")),
        (Err(_), _) => Outcome::LegacyPanicked,
    }
}

fn outcome_diff(id: &str, name: &str, cfg: &Value) -> Option<String> {
    match outcome(id, name, cfg) {
        Outcome::Differ(d) | Outcome::Stricter(d) => Some(d),
        _ => None,
    }
}

fn compare_outcomes(id: &str, name: &str, cfg: &Value) {
    if let Some(d) = outcome_diff(id, name, cfg) {
        panic!("{d}");
    }
}

/// Single-key mutants of a configuration: every key deleted, nulled, flipped (bool), nudged
/// (numbers), rewritten (strings), shortened (arrays) — at the top level and one object down —
/// plus one unknown key. A mutant is usually not a real model; it is a *probe*: the adapter and the
/// Rust parser must still agree on it (both read it into the same spec, or both refuse it).
fn mutants(cfg: &Value) -> Vec<(String, Value)> {
    fn variants(v: &Value) -> Vec<(String, Value)> {
        let mut out = vec![("null".to_string(), Value::Null)];
        match v {
            Value::Bool(b) => out.push(("flip".into(), Value::Bool(!b))),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    out.push(("+1".into(), Value::from(i + 1)));
                    if i >= 1 {
                        out.push(("-1".into(), Value::from(i - 1)));
                    }
                    out.push(("0".into(), Value::from(0)));
                } else if let Some(f) = n.as_f64() {
                    out.push(("x2".into(), serde_json::Number::from_f64(f * 2.0).map(Value::Number).unwrap_or(Value::Null)));
                    out.push(("x0.5".into(), serde_json::Number::from_f64(f * 0.5).map(Value::Number).unwrap_or(Value::Null)));
                }
            }
            Value::String(_) => {
                for s in ["silu", "gelu", "relu", "bogus"] {
                    out.push((format!("str:{s}"), Value::String(s.into())));
                }
            }
            Value::Array(a) if !a.is_empty() => {
                out.push(("drop-last".into(), Value::Array(a[..a.len() - 1].to_vec())));
                if a.iter().all(Value::is_string) {
                    out.push(("first=full".into(), Value::Array(std::iter::once(Value::String("full_attention".into())).chain(a[1..].iter().cloned()).collect())));
                    out.push(("first=sliding".into(), Value::Array(std::iter::once(Value::String("sliding_attention".into())).chain(a[1..].iter().cloned()).collect())));
                    out.push(("first=bogus".into(), Value::Array(std::iter::once(Value::String("bogus".into())).chain(a[1..].iter().cloned()).collect())));
                }
            }
            _ => {}
        }
        out
    }
    let mut out = Vec::new();
    let Value::Object(top) = cfg else { return out };
    for (k, v) in top {
        if k == "architectures" {
            continue;
        }
        let mut del = cfg.clone();
        del.as_object_mut().map(|o| o.remove(k));
        out.push((format!("-{k}"), del));
        for (what, nv) in variants(v) {
            let mut m = cfg.clone();
            m[k] = nv;
            out.push((format!("{k}={what}"), m));
        }
        if let Value::Object(inner) = v {
            for (k2, v2) in inner {
                let mut del = cfg.clone();
                del[k].as_object_mut().map(|o| o.remove(k2));
                out.push((format!("-{k}.{k2}"), del));
                for (what, nv) in variants(v2) {
                    let mut m = cfg.clone();
                    m[k][k2] = nv;
                    out.push((format!("{k}.{k2}={what}"), m));
                }
            }
        }
    }
    let mut unk = cfg.clone();
    unk["zzz_unknown_key"] = Value::from(1);
    out.push(("+zzz_unknown_key".into(), unk));
    out
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

/// **The adapters agree with the Rust parsers off the fixtures too.** Every fixture and real config
/// is probed with its single-key mutants (see [`mutants`]); the two readers must agree on each.
#[test]
fn adapters_agree_with_the_rust_parsers_on_single_key_mutants() {
    // A debug build reads ~30 mutants a second; the default samples every 24th, and
    // `PALW_MUTANT_STRIDE=1 cargo test --release …` runs them all (15 k probes).
    let stride: usize = std::env::var("PALW_MUTANT_STRIDE").ok().and_then(|v| v.parse().ok()).filter(|s| *s >= 1).unwrap_or(24);
    // `PALW_MUTANT_ONLY=deepseek-v3,phimoe` probes just those adapters.
    let only: Vec<String> = std::env::var("PALW_MUTANT_ONLY").ok().map(|v| v.split(',').map(str::to_string).collect()).unwrap_or_default();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let (mut probes, mut read, mut refused, mut legacy_panics, mut k) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut bad: Vec<String> = Vec::new();
    let mut stricter: Vec<String> = Vec::new();
    for (id, name, cfg) in pairs() {
        if !only.is_empty() && !only.contains(&id) {
            continue;
        }
        for (what, m) in mutants(&cfg) {
            k += 1;
            if k % stride != 0 {
                continue;
            }
            probes += 1;
            match outcome(&id, &format!("{name} [{what}]"), &m) {
                Outcome::Differ(d) => bad.push(d),
                Outcome::Stricter(d) => stricter.push(d),
                Outcome::Agree { read: true } => read += 1,
                Outcome::Agree { read: false } => refused += 1,
                Outcome::LegacyPanicked => legacy_panics += 1,
            }
        }
    }
    std::panic::set_hook(hook);
    eprintln!(
        "{probes} mutants (stride {stride}): {read} read identically, {refused} refused by both, {} refused by the adapter's generic checks only, {legacy_panics} on which only the Rust parser panics, {} disagree",
        stricter.len(),
        bad.len()
    );
    for d in stricter.iter().take(3) {
        eprintln!("  stricter, e.g. {d}");
    }
    for d in bad.iter().take(400) {
        eprintln!("  {d}");
    }
    assert!(bad.is_empty(), "{} mutants on which an adapter and the Rust parser disagree", bad.len());
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
