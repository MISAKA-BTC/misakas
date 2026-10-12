//! **The vision-tower adapters are the Rust reader's data form** (FR-19, the towers that used to be Rust only): on every
//! fixture and every real configuration, and on single-key mutants of them, the adapters of kind `vision` instantiate the
//! same `VisionSpec` (every field, every tensor name) as `parse_vision_rust`, the reader they replaced — CLIP, SigLIP,
//! Qwen2-VL, Qwen2.5-VL and LLaVA. A wrapper model (a VLM) is read as its text decoder; its tower is reached by name
//! (`match.tower_of`), and a tower's own unknown key is refused while a wrapper's is not this adapter's business.

use misaka_palw_tir_lower::adapter::builtin;
use misaka_palw_tir_lower::hf_schema::is_vision_tower;
use misaka_palw_tir_lower::lower::vision::{parse_vision, parse_vision_rust};
use serde_json::{Value, json};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// One configuration under test: its name, text, the class's declared input size and the processor's numbers (when the case
/// has them).
struct Case {
    name: String,
    text: String,
    size: Option<(u32, u32)>,
    ms: Option<([f64; 3], [f64; 3])>,
}

fn arr3(v: &Value) -> [f64; 3] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap()]
}

fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    // The fixtures: their configuration, the size and normalisation their generator used (the class's, not the config's).
    for name in ["clip_vision", "siglip_vision", "qwen2_vl_vision", "qwen2_5_vl_vision", "qwen2_vl", "qwen2_5_vl", "llava"] {
        let dir = tests_dir().join("fixtures/hf-vis").join(name);
        let text = std::fs::read_to_string(dir.join("config.json")).expect("config");
        let o: Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).expect("outputs")).expect("json");
        // Only the Qwen towers take the class's size (their config has none); CLIP, SigLIP and LLaVA read `image_size`.
        let size = name.starts_with("qwen").then(|| (o["size"][0].as_u64().unwrap() as u32, o["size"][1].as_u64().unwrap() as u32));
        let ms = Some((arr3(&o["mean"]), arr3(&o["std"])));
        v.push(Case { name: format!("fixture {name}"), text: text.clone(), size, ms });
        // Without the processor's numbers: each family's own default normalisation.
        v.push(Case { name: format!("fixture {name}, defaults"), text, size, ms: None });
    }
    // The real configurations: the published tower inside its wrapper, at the usual 224 × 224.
    for name in ["qwen2-vl-2b-instruct", "qwen2-vl-7b-instruct", "qwen2.5-vl-3b-instruct", "qwen2.5-vl-7b-instruct"] {
        let text = std::fs::read_to_string(tests_dir().join("configs/vision").join(format!("{name}.json"))).expect("config");
        v.push(Case { name: format!("real {name}"), text, size: Some((224, 224)), ms: None });
    }
    let llava = std::fs::read_to_string(tests_dir().join("configs/real/llava-1.5-7b-hf.json")).expect("config");
    v.push(Case { name: "real llava-1.5-7b-hf".into(), text: llava, size: None, ms: None });
    v
}

fn spec_json(r: Result<misaka_palw_tir_lower::lower::vision::VisionSpec, misaka_palw_tir_lower::LowerError>) -> Result<Value, String> {
    r.map(|s| serde_json::to_value(&s).unwrap()).map_err(|e| e.to_string())
}

#[test]
fn every_tower_adapter_reads_what_the_rust_reader_read() {
    for c in cases() {
        let a = spec_json(parse_vision(&c.text, c.size, c.ms)).unwrap_or_else(|e| panic!("{}: the adapter refused: {e}", c.name));
        let r = spec_json(parse_vision_rust(&c.text, c.size, c.ms)).unwrap_or_else(|e| panic!("{}: the Rust reader refused: {e}", c.name));
        assert_eq!(a, r, "{}", c.name);
    }
}

/// What the two readers said about one configuration.
enum Outcome {
    Same,
    /// The adapter refuses where the Rust reader read the configuration without looking (a value no Hugging Face class runs).
    Stricter,
    Differ(String),
}

fn outcome(label: &str, text: &str, size: Option<(u32, u32)>, ms: Option<([f64; 3], [f64; 3])>) -> Outcome {
    let rust = catch_unwind(AssertUnwindSafe(|| spec_json(parse_vision_rust(text, size, ms))));
    let adapter = match catch_unwind(AssertUnwindSafe(|| spec_json(parse_vision(text, size, ms)))) {
        Ok(a) => a,
        Err(_) => return Outcome::Differ(format!("{label}: the adapter PANICS")),
    };
    match (rust, adapter) {
        (Ok(Ok(r)), Ok(a)) if r == a => Outcome::Same,
        (Ok(Ok(r)), Ok(a)) => Outcome::Differ(format!("{label}: read differently\n  rust    {r}\n  adapter {a}")),
        (Ok(Err(_)), Err(_)) => Outcome::Same,
        (Ok(Ok(_)), Err(_)) | (Err(_), Err(_)) | (Err(_), Ok(_)) => Outcome::Stricter,
        (Ok(Err(e)), Ok(_)) => Outcome::Differ(format!("{label}: the adapter reads what the Rust reader refused ({e})")),
    }
}

/// Every key of every configuration — of the wrapper, of its `vision_config`, and the text width of a LLaVA projector —
/// deleted, nulled, flipped, nudged or rewritten: the adapter and the Rust reader either read the same spec or both refuse,
/// and the adapter is never LOOSER. (It may be stricter: the Rust reader looked only at the keys it needs.)
#[test]
fn single_key_mutants_agree_or_the_adapter_is_stricter() {
    let variants: Vec<(&str, Value)> = vec![
        ("null", Value::Null),
        ("zero", json!(0)),
        ("one", json!(1)),
        ("two", json!(2)),
        ("minus one", json!(-1)),
        ("minus two", json!(-2)),
        ("half", json!(0.5)),
        ("big", json!(1_000_000_000_000u64)),
        ("true", json!(true)),
        ("false", json!(false)),
        ("text", json!("x")),
        ("empty list", json!([])),
        ("a list", json!([1, 3])),
        ("relu", json!("relu")),
        ("gelu", json!("gelu")),
        ("full", json!("full")),
    ];
    let (mut same, mut stricter, mut bad) = (0usize, 0usize, Vec::new());
    // The Rust reader divides by what a mutant zeroes: those panics are an outcome here, not noise.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for c in cases() {
        let cfg: Value = serde_json::from_str(&c.text).unwrap();
        // Paths to mutate: every top-level key, every key of `vision_config`, and `text_config.hidden_size`.
        let mut paths: Vec<Vec<String>> = cfg.as_object().unwrap().keys().map(|k| vec![k.clone()]).collect();
        if let Some(v) = cfg.get("vision_config").and_then(Value::as_object) {
            paths.extend(v.keys().map(|k| vec!["vision_config".to_string(), k.clone()]));
        }
        if cfg.get("text_config").is_some() {
            paths.push(vec!["text_config".into(), "hidden_size".into()]);
        }
        for path in &paths {
            let at = |root: &mut Value| -> (*mut Value, String) {
                let mut cur = root;
                for p in &path[..path.len() - 1] {
                    cur = cur.get_mut(p.as_str()).expect("parent");
                }
                (cur as *mut Value, path.last().unwrap().clone())
            };
            let mut muts: Vec<(String, Value)> = Vec::new();
            let mut del = cfg.clone();
            let (parent, key) = at(&mut del);
            // SAFETY: `parent` points into `del`, which is alive and not otherwise borrowed here.
            unsafe { (*parent).as_object_mut().expect("object").remove(&key) };
            muts.push((format!("{}: delete {}", c.name, path.join(".")), del));
            for (vn, v) in &variants {
                let mut m = cfg.clone();
                let (parent, key) = at(&mut m);
                // SAFETY: as above.
                unsafe { (*parent).as_object_mut().expect("object").insert(key, v.clone()) };
                muts.push((format!("{}: {} = {vn}", c.name, path.join(".")), m));
            }
            for (label, m) in muts {
                match outcome(&label, &m.to_string(), c.size, c.ms) {
                    Outcome::Same => same += 1,
                    Outcome::Stricter => stricter += 1,
                    Outcome::Differ(why) => bad.push(why),
                }
            }
        }
    }
    std::panic::set_hook(hook);
    eprintln!("{same} mutants read the same, {stricter} the adapter refuses where the Rust reader did not");
    assert!(bad.is_empty(), "{} mutants on which the adapter and the Rust reader disagree:\n{}", bad.len(), bad.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
    assert!(same > 1500, "{same}");
}

/// A VLM wrapper is a TEXT model for the reader (its tower is a component reached by name): the report of a wrapper stays the
/// text decoder's, and only a tower alone is "a vision tower".
#[test]
fn a_wrapper_is_not_a_vision_tower_but_its_tower_is_reachable_by_name() {
    for (wrapper, adapter) in [
        ("Qwen2VLForConditionalGeneration", "qwen2-vl-vision-in-vlm"),
        ("Qwen2_5_VLForConditionalGeneration", "qwen2-5-vl-vision-in-vlm"),
        ("LlavaForConditionalGeneration", "llava-vision"),
    ] {
        assert!(!is_vision_tower(&json!({"architectures": [wrapper]})), "{wrapper} is a wrapper, not a tower");
        assert!(builtin::find_vision_for(wrapper, None).is_none(), "{wrapper}: claimed as a tower alone");
        assert_eq!(builtin::find_tower_in(wrapper).map(|a| a.id.as_str()), Some(adapter), "{wrapper}");
    }
    for (tower, adapter) in [("Qwen2VisionTransformerPretrainedModel", "qwen2-vl-vision"), ("Qwen2_5_VisionTransformerPretrainedModel", "qwen2-5-vl-vision")] {
        assert!(is_vision_tower(&json!({"architectures": [tower]})), "{tower}");
        assert_eq!(builtin::find_vision_for(tower, None).map(|a| a.id.as_str()), Some(adapter));
    }
}

/// The tower's own unknown key is refused by name (a key nobody accounts for might change the math); the wrapper's other keys
/// are the text decoder's adapter's to account for, so they pass here.
#[test]
fn a_towers_unknown_key_is_refused_never_ignored_and_the_wrappers_is_not_its_business() {
    for name in ["qwen2_vl", "qwen2_5_vl", "llava"] {
        let text = std::fs::read_to_string(tests_dir().join("fixtures/hf-vis").join(name).join("config.json")).unwrap();
        let mut cfg: Value = serde_json::from_str(&text).unwrap();
        cfg["a_wrapper_key_nobody_has_heard_of"] = json!(3);
        parse_vision(&cfg.to_string(), Some((28, 28)), None).unwrap_or_else(|e| panic!("{name}: a wrapper key refused by the tower's adapter: {e}"));
        cfg["vision_config"]["a_tower_key_nobody_has_heard_of"] = json!(3);
        let e = parse_vision(&cfg.to_string(), Some((28, 28)), None).unwrap_err().to_string();
        assert!(e.contains("a_tower_key_nobody_has_heard_of"), "{name}: {e}");
        assert!(parse_vision_rust(&cfg.to_string(), Some((28, 28)), None).is_ok(), "{name}: the Rust reader never looked");
    }
}

/// What the Rust reader refused, by name: LLaVA's `vision_feature_select_strategy` other than `default`; a Qwen tower with no
/// declared input size; a wrapper with no `vision_config`.
#[test]
fn the_refusals_of_the_rust_reader_are_refused_by_the_adapters() {
    let text = std::fs::read_to_string(tests_dir().join("fixtures/hf-vis/llava/config.json")).unwrap();
    let mut cfg: Value = serde_json::from_str(&text).unwrap();
    cfg["vision_feature_select_strategy"] = json!("full");
    let e = parse_vision(&cfg.to_string(), None, None).unwrap_err().to_string();
    assert!(e.contains("vision_feature_select_strategy"), "{e}");
    assert!(parse_vision_rust(&cfg.to_string(), None, None).is_err());
    let text = std::fs::read_to_string(tests_dir().join("fixtures/hf-vis/qwen2_vl/config.json")).unwrap();
    let e = parse_vision(&text, None, None).unwrap_err().to_string();
    assert!(e.contains("input size"), "{e}");
    let mut cfg: Value = serde_json::from_str(&text).unwrap();
    cfg.as_object_mut().unwrap().remove("vision_config");
    let e = parse_vision(&cfg.to_string(), Some((28, 28)), None).unwrap_err().to_string();
    assert!(e.contains("vision_config"), "{e}");
}
