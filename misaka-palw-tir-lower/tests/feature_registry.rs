//! **The feature vocabulary is honest** (`ModelSpec` V1, `src/model/features.rs`).
//!
//! * every feature a fixture's spec uses is in the registry and implemented;
//! * the primitives a lowered program uses are declared by its features (plus the base set): a
//!   registry that under-declares what a lowering emits fails here;
//! * the registry is well formed: stable-looking ids, no duplicates, and for implemented features
//!   test ids that name a test file and a test (or fixture) that exists.

use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::lower::LowerOpts;
use misaka_palw_tir_lower::model::{BASE_PRIMITIVES, Lowering, REGISTRY, Requirement, feature_info};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every fixture checkpoint and real config this crate lowers, by name.
fn corpus() -> Vec<(String, String)> {
    let mut v = Vec::new();
    for root in ["tests/fixtures/hf", "tests/fixtures/hf-quant", "tests/fixtures/gguf"] {
        let dir = crate_dir().join(root);
        let mut names: Vec<PathBuf> = std::fs::read_dir(&dir).expect("fixtures").map(|e| e.expect("entry").path()).collect();
        names.sort();
        for d in names {
            let name = d.file_name().unwrap_or_default().to_string_lossy().to_string();
            let cfg = if d.join("config.json").exists() {
                std::fs::read_to_string(d.join("config.json")).expect("config")
            } else {
                continue;
            };
            v.push((format!("{}/{name}", root.rsplit('/').next().unwrap_or(root)), cfg));
        }
    }
    v
}

fn real_configs() -> Vec<(String, String)> {
    let dir = crate_dir().join("tests/configs/real");
    let mut v: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("configs")
        .map(|e| e.expect("entry").path())
        .map(|p| (p.file_stem().unwrap_or_default().to_string_lossy().to_string(), std::fs::read_to_string(&p).expect("config")))
        .collect();
    v.sort();
    v
}

#[test]
fn every_feature_a_lowered_fixture_uses_is_in_the_registry_and_implemented() {
    let mut bad = Vec::new();
    let mut seen: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (name, cfg) in corpus() {
        let spec = match misaka_palw_tir_lower::parse_config_str(&cfg) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{name}: not parsed ({e})");
                continue;
            }
        };
        for u in spec.features() {
            match feature_info(u.id.0) {
                None => bad.push(format!("{name}: {} is not in the registry", u.id)),
                Some(i) => {
                    *seen.entry(i.id.0).or_default() += 1;
                    if i.lowering != Lowering::Implemented || i.protocol != Requirement::None {
                        bad.push(format!("{name}: {} is {:?}/{:?}", u.id, i.lowering, i.protocol));
                    }
                }
            }
        }
    }
    let unused: Vec<&str> = REGISTRY.iter().filter(|f| f.lowering == Lowering::Implemented && !seen.contains_key(f.id.0)).map(|f| f.id.0).collect();
    eprintln!("{} features used by the corpus; implemented but unused by any fixture: {unused:?}", seen.len());
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn a_lowered_program_uses_only_primitives_its_features_declare() {
    let mut bad = Vec::new();
    let mut tight: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (name, cfg) in corpus().into_iter().filter(|(n, _)| n.starts_with("hf/")) {
        let Ok(prep) = fidelity::prepare(&cfg, &LowerOpts::default()) else { continue };
        let used: BTreeSet<&str> = prep.lowered.program.blocks.iter().flat_map(|b| b.nodes.iter().map(|n| n.prim.name())).collect();
        let mut declared: BTreeSet<&str> = BASE_PRIMITIVES.iter().copied().collect();
        for u in prep.spec.features() {
            declared.extend(feature_info(u.id.0).map(|i| i.primitives.iter().copied()).into_iter().flatten());
        }
        let undeclared: Vec<&&str> = used.difference(&declared).collect();
        if !undeclared.is_empty() {
            bad.push(format!("{name}: uses {undeclared:?} that none of its features declares"));
        }
        // What each feature of this model never needed (informational: how loose the lists are).
        for u in prep.spec.features() {
            if let Some(i) = feature_info(u.id.0) {
                for p in i.primitives {
                    if !used.contains(p) {
                        tight.entry(i.id.0).or_default().insert(format!("{p} (in {name})"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn the_registry_is_well_formed() {
    let mut ids = BTreeSet::new();
    for f in REGISTRY {
        assert!(ids.insert(f.id.0), "duplicate {}", f.id);
        let ok = f.id.0.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && f.id.0.rsplit('_').next().is_some_and(|v| v.starts_with('V') && v[1..].chars().all(|c| c.is_ascii_digit()) && v.len() > 1);
        assert!(ok, "{}: ids are UPPER_SNAKE_V<n>", f.id);
        assert!(!f.title.is_empty());
        for p in f.primitives {
            assert!(misaka_palw_tir::prim::PRIM_NAMES_V1.contains(p), "{}: `{p}` is not one of the 25 primitives", f.id);
        }
        if let Requirement::Capability { id, general_primitive, .. } = f.protocol {
            assert!(!id.is_empty() && !general_primitive.is_empty(), "{}: a capability names itself and its general closing primitive", f.id);
        }
    }
}

/// An implemented feature's test ids name a test file of this crate and a test or fixture in it.
#[test]
fn an_implemented_features_tests_exist() {
    let mut missing = Vec::new();
    for f in REGISTRY.iter().filter(|f| f.lowering == Lowering::Implemented) {
        assert!(!f.tests.is_empty(), "{} has no test id", f.id);
        for t in f.tests {
            let Some((file, name)) = t.split_once("::") else {
                missing.push(format!("{}: `{t}` is not <file>::<name>", f.id));
                continue;
            };
            if file == "library" {
                continue; // misaka-palw-tir's own tests
            }
            let p = crate_dir().join("tests").join(format!("{file}.rs"));
            let Ok(text) = std::fs::read_to_string(&p) else {
                missing.push(format!("{}: no test file tests/{file}.rs", f.id));
                continue;
            };
            let fixture = ["hf", "hf-enc", "hf-quant", "gguf", "hf-lora"].iter().any(|r| Path::new(&crate_dir().join("tests/fixtures").join(r).join(name)).exists());
            if !text.contains(name) && !fixture {
                missing.push(format!("{}: tests/{file}.rs names no `{name}`", f.id));
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

/// Per feature: the primitives always present, and sometimes present, over the fixtures that use
/// it (a drafting aid for the registry's lists: `cargo test … -- --ignored --nocapture`).
#[test]
#[ignore]
fn print_primitives_by_feature() {
    use std::collections::BTreeMap;
    let mut always: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut some: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let base: BTreeSet<String> = BASE_PRIMITIVES.iter().map(|s| s.to_string()).collect();
    for (name, cfg) in corpus().into_iter().filter(|(n, _)| n.starts_with("hf/")) {
        let Ok(prep) = fidelity::prepare(&cfg, &LowerOpts::default()) else { continue };
        let used: BTreeSet<String> = prep.lowered.program.blocks.iter().flat_map(|b| b.nodes.iter().map(|n| n.prim.name().to_string())).collect();
        let extra: BTreeSet<String> = used.difference(&base).cloned().collect();
        eprintln!("{name:>24}: {}", extra.iter().cloned().collect::<Vec<_>>().join(" "));
        for u in prep.spec.features() {
            let a = always.entry(u.id.0).or_insert_with(|| extra.clone());
            *a = a.intersection(&extra).cloned().collect();
            some.entry(u.id.0).or_default().extend(extra.iter().cloned());
        }
    }
    for (f, a) in &always {
        eprintln!("{f:>34} always [{}]", a.iter().cloned().collect::<Vec<_>>().join(" "));
    }
}

/// A feature's declared primitives are not decoration: each is emitted by the lowering of some
/// fixture that uses the feature (so the registry cannot claim a primitive nothing ever produces).
#[test]
fn a_declared_primitive_is_emitted_by_some_fixture_that_uses_the_feature() {
    let mut union: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    let mut fixtures_using: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (_, cfg) in corpus().into_iter().filter(|(n, _)| n.starts_with("hf/")) {
        let Ok(prep) = fidelity::prepare(&cfg, &LowerOpts::default()) else { continue };
        let used: BTreeSet<String> = prep.lowered.program.blocks.iter().flat_map(|b| b.nodes.iter().map(|n| n.prim.name().to_string())).collect();
        for u in prep.spec.features() {
            union.entry(u.id.0).or_default().extend(used.iter().cloned());
            *fixtures_using.entry(u.id.0).or_default() += 1;
        }
    }
    let mut bad = Vec::new();
    for f in REGISTRY.iter().filter(|f| f.lowering == Lowering::Implemented) {
        let Some(u) = union.get(f.id.0) else { continue };
        for p in f.primitives {
            if !u.contains(*p) {
                bad.push(format!("{} declares {p}, which no fixture using it emits", f.id));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
