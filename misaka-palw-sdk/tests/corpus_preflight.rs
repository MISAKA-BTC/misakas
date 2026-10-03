//! **A golden preflight for every entry of the architecture corpus** (RFC-0002 Part II §II.8, R1 over the corpus v2 manifest of
//! `misaka-palw-tir-lower/tools/corpus`): each of the 100 curated architectures is preflighted at the `shape` depth on testnet-12 from
//! its committed light spec (`config.json` and the tensors' names, dtypes and shapes — written back as header-only safetensors, the
//! form a ranged download of a real repository gives), with the third-party adapter the corpus carries for it where there is one, and
//! its verdict — each stage's status and the blockers' codes — is pinned in `tests/golden/corpus_preflight_v1.json`.
//!
//! The pins hold the *claim a user would be shown* steady: a change to a feature, an adapter, a check or a code that moves one model's
//! verdict fails here by name (`UPDATE_PINS=1` rewrites the file; review the diff). They also record the coverage the corpus measures
//! on the preflight's own terms: how many of the 100 reach `convert: ok`.

use misaka_palw_sdk::preflight::{self, Depth, Options};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn lower() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower")
}

fn dtype_bytes(d: &str) -> u64 {
    match d {
        "F64" | "I64" | "U64" => 8,
        "F32" | "I32" | "U32" => 4,
        "F16" | "BF16" | "I16" | "U16" => 2,
        _ => 1,
    }
}

/// A safetensors file holding only its header (the tensors' data are not there: a ranged download of the header's prefix).
fn write_header_only(dir: &Path, tensors: &serde_json::Map<String, Value>) {
    let mut header = serde_json::Map::new();
    let mut at = 0u64;
    for (name, t) in tensors {
        let dtype = t["dtype"].as_str().unwrap_or("F32");
        let n: u64 = t["shape"].as_array().map_or(1, |s| s.iter().filter_map(Value::as_u64).product());
        let bytes = n * dtype_bytes(dtype);
        header.insert(name.clone(), json!({"dtype": dtype, "shape": t["shape"], "data_offsets": [at, at + bytes]}));
        at += bytes;
    }
    let text = serde_json::to_vec(&Value::Object(header)).expect("header");
    let mut file = (text.len() as u64).to_le_bytes().to_vec();
    file.extend(text);
    std::fs::write(dir.join("model.safetensors"), file).expect("write the header");
}

fn verdict_of(id: &str) -> Value {
    let spec = lower().join("tools/corpus/specs").join(id);
    let Ok(config) = std::fs::read_to_string(spec.join("config.json")) else { return json!({"error": "no light spec"}) };
    let tmp = std::env::temp_dir().join(format!("corpus-preflight-{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("tmp");
    std::fs::write(tmp.join("config.json"), config).expect("config");
    let tensors: serde_json::Map<String, Value> = std::fs::read_to_string(spec.join("tensors.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let mut heads = tmp.join("headers");
    std::fs::create_dir_all(&heads).expect("headers");
    write_header_only(&heads, &tensors);
    // A diagnosis: `CORPUS_HEAVY=<dir>` reads the real checkpoint's headers instead of the light spec's (when the entry has one).
    if let Some(h) = std::env::var_os("CORPUS_HEAVY").map(|d| PathBuf::from(d).join(id)).filter(|d| d.join("model.safetensors").exists()) {
        heads = h;
    }
    let adapter = lower().join("tools/corpus/adapters").join(format!("{id}.json"));
    // What a user does: preflight as is (the built-in pack reads what it claims); when that does not reach `convert: ok` and the corpus
    // carries a third-party adapter file for the entry, preflight again with it (a built-in adapter newer than the file wins by being tried first).
    let run = |with: Option<PathBuf>| {
        let opts = Options {
            depth: if std::env::var_os("CORPUS_HEADERS").is_some() { Depth::Headers } else { Depth::Shape },
            network: Some("testnet-12".into()),
            headers: Some(heads.clone()),
            adapter: with,
            // The context is declared (128 positions): the verdict is then a function of the model, not of the widest context the gate would try,
            // whose close-size walk takes minutes on some programs (internlm2: ~3.5 min in release at 128 already).
            max_context: Some(std::env::var("CORPUS_CTX").ok().and_then(|v| v.parse().ok()).unwrap_or(128)),
            ..Options::default()
        };
        preflight::run(&tmp.join("config.json"), &opts)
    };
    let mut result = run(None);
    let mut with_adapter = false;
    if adapter.exists() && result.as_ref().map_or(true, |r| r.verdict.convert.status != preflight::StageStatus::Ok) {
        result = run(Some(adapter));
        with_adapter = true;
    }
    let mut out = match result {
        Ok(r) => {
            let stage = |v: &preflight::StageVerdict| {
                json!({
                    "status": serde_json::to_value(v.status).expect("status"),
                    "blockers": v.blockers.iter().map(|b| match &b.arg { Some(a) => format!("{}({a})", b.code), None => b.code.clone() }).collect::<Vec<_>>(),
                })
            };
            json!({"convert": stage(&r.verdict.convert), "register": stage(&r.verdict.register), "mine": stage(&r.verdict.mine)})
        }
        Err(e) => json!({"error": e.chars().take(160).collect::<String>()}),
    };
    if with_adapter {
        out["adapter_file"] = json!(true);
    }
    let _ = std::fs::remove_dir_all(&tmp);
    out
}

#[test]
fn every_corpus_entry_has_its_pinned_preflight() {
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(lower().join("tools/corpus/corpus_v2.json")).expect("manifest")).expect("json");
    let ids: Vec<String> = manifest["entries"].as_array().expect("entries").iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect();
    assert!((50..=100).contains(&ids.len()), "{} entries", ids.len());
    // `CORPUS_ONLY=a,b` runs a subset (a diagnosis; the pins are compared for those only).
    let only: Option<Vec<String>> = std::env::var("CORPUS_ONLY").ok().map(|v| v.split(',').map(str::to_string).collect());
    let now: BTreeMap<String, Value> = ids
        .iter()
        .filter(|id| only.as_ref().is_none_or(|o| o.contains(id)))
        .map(|id| {
            let t = std::time::Instant::now();
            let v = verdict_of(id);
            eprintln!("{id:>22}: {:>6.1}s {}", t.elapsed().as_secs_f64(), v["convert"]["status"]);
            (id.clone(), v)
        })
        .collect();
    let pins = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/corpus_preflight_v1.json");
    if std::env::var_os("UPDATE_PINS").is_some() {
        std::fs::create_dir_all(pins.parent().expect("dir")).expect("golden dir");
        std::fs::write(&pins, serde_json::to_string_pretty(&now).expect("json") + "\n").expect("write the pins");
        return;
    }
    let pinned: BTreeMap<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(&pins).expect("tests/golden/corpus_preflight_v1.json (UPDATE_PINS=1 writes it)")).expect("pins");
    for (id, v) in &now {
        assert_eq!(pinned.get(id), Some(v), "{id}: its preflight verdict moved (UPDATE_PINS=1 after review)");
    }
    assert!(only.is_some() || pinned.len() == now.len(), "an entry was added or dropped");
}

/// The coverage the preflight itself reports, over the corpus: how many reach `convert: ok`.
#[test]
fn the_corpus_coverage_on_the_preflights_terms() {
    let pins = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/corpus_preflight_v1.json");
    let Ok(text) = std::fs::read_to_string(&pins) else { return };
    let pinned: BTreeMap<String, Value> = serde_json::from_str(&text).expect("pins");
    let ok = pinned.values().filter(|v| v["convert"]["status"] == "ok").count();
    eprintln!("corpus entries that reach convert: ok on the preflight: {ok} of {}", pinned.len());
    assert!(!pinned.is_empty());
}
