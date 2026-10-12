//! A small untrusted frontend must not allocate its exponentially expanded result before refusal.
use misaka_palw_tir_lower::adapter::{
    self, Origin,
    expr::{EXPANSION_LIMIT, Env, MAX_DEPTH, MAX_LIST},
};
use misaka_palw_tir_lower::cfg::Cfg;
use serde_json::{Map, Value, json};

fn evaluate(value: Value) -> Result<Value, String> {
    let config = Map::new();
    let defaults = Map::new();
    let cfg = Cfg::new("third-party", &config, "");
    Env::new(&cfg, None, &defaults, None, "third-party").eval(&value).map_err(|e| e.to_string())
}

fn refused(value: Value) {
    let err = evaluate(value).expect_err("expansion is refused before allocation");
    assert!(err.contains(EXPANSION_LIMIT), "{err}");
}

#[test]
fn nested_repeat_accounts_for_the_subtree_not_only_the_outer_list() {
    // Both list lengths are individually legal; the requested result is 2^30 Value entries.
    refused(json!({"$repeat": [{"$range": 1024}, MAX_LIST]}));
}

#[test]
fn string_bytes_and_object_keys_are_part_of_the_expansion_budget() {
    refused(json!({"$repeat": ["x".repeat(65_536), 4096]}));
    let mut object = Map::new();
    object.insert("k".repeat(65_536), json!(null));
    refused(json!({"$repeat": [object, 4096]}));
}

#[test]
fn copying_a_cached_variable_cannot_reset_the_budget() {
    let config = Map::new();
    let defaults = Map::new();
    let cfg = Cfg::new("third-party", &config, "");
    let env = Env::new(&cfg, None, &defaults, None, "third-party");
    env.define_global("a", json!({"$range": 4096}));
    let err = env.eval(&json!({"$repeat": [{"$var": "a"}, 4096]})).unwrap_err().to_string();
    assert!(err.contains(EXPANSION_LIMIT), "{err}");
}

#[test]
fn lazy_layer_templates_are_bounded_even_when_the_body_does_not_read_them() {
    let config = Map::new();
    let defaults = Map::new();
    let cfg = Cfg::new("third-party", &config, "");
    let env = Env::new(&cfg, None, &defaults, None, "third-party");
    env.add_layer_var("unused", json!("x".repeat(65_536)), false);
    let err = env.eval(&json!({"$layers": {"count": 4096, "each": 1}})).unwrap_err().to_string();
    assert!(err.contains(EXPANSION_LIMIT), "{err}");
}

#[test]
fn variable_indirection_does_not_reset_recursion_depth() {
    let config = Map::new();
    let defaults = Map::new();
    let cfg = Cfg::new("third-party", &config, "");
    let env = Env::new(&cfg, None, &defaults, None, "third-party");
    for n in 0..2 * MAX_DEPTH {
        env.define_global(&format!("v{n}"), json!({"$var": format!("v{}", n + 1)}));
    }
    env.define_global(&format!("v{}", 2 * MAX_DEPTH), json!(1));
    let err = env.eval(&json!({"$var": "v0"})).unwrap_err().to_string();
    assert!(err.contains(EXPANSION_LIMIT), "{err}");
    assert_eq!(env.eval(&json!(7)).unwrap(), json!(7), "depth is unwound after refusal");
}

#[test]
fn nested_operators_refuse_on_the_default_worker_stack() {
    // This is below serde_json's parse limit; the interpreter must refuse without relying on
    // a caller to allocate a specially enlarged stack.
    let mut value = json!(1);
    for _ in 0..64 {
        value = json!({"$repeat": [value, 1]});
    }
    refused(value);
}

#[test]
fn generated_lists_and_quadratic_operators_spend_work_budget() {
    refused(json!({"$alibi": {"heads": MAX_LIST + 1, "kind": "bloom"}}));
    assert!(evaluate(json!({"$alibi": {"heads": u64::MAX, "kind": "bloom"}})).is_err());
    refused(json!({"$range": MAX_LIST + 1}));
    refused(json!({"$unique": {"$range": 5000}}));
    refused(json!({"$map": [{"$range": 4096}, "i", {"$repeat": ["data", 4096]}]}));
}

#[test]
fn ordinary_compositions_preserve_their_exact_values() {
    assert_eq!(evaluate(json!({"$repeat": [[1, 2, 3], 2]})).unwrap(), json!([[1, 2, 3], [1, 2, 3]]));
    assert_eq!(evaluate(json!({"$flatten": {"$repeat": [[1, 2], 3]}})).unwrap(), json!([1, 2, 1, 2, 1, 2]));
    assert_eq!(evaluate(json!({"$concat": [[1, 2], [3], []]})).unwrap(), json!([1, 2, 3]));
    assert_eq!(evaluate(json!({"$unique": [1, 2, 1, 3, 2]})).unwrap(), json!([1, 2, 3]));
    assert_eq!(evaluate(json!({"$cat": ["layer.", 2, ".weight"]})).unwrap(), json!("layer.2.weight"));
    assert_eq!(evaluate(json!({"$map": [{"$range": 3}, "i", {"$mul": [{"$var": "i"}, 2]}]})).unwrap(), json!([0, 2, 4]));
}

#[test]
fn public_adapter_entry_point_refuses_a_tiny_expansion_bomb() {
    let source = json!({
        "format": adapter::ADAPTER_FORMAT_V1,
        "id": "independent-compiler",
        "vars": [{"name": "bomb", "value": {"$repeat": [{"$range": 1024}, 1_048_576]}}],
        "spec": {}
    })
    .to_string();
    assert!(source.len() < 512);
    let adapter = adapter::parse(&source, Origin::User).unwrap();
    let err = adapter::eval::build_spec(&adapter, &json!({"architectures": ["PreviouslyUnknown"]}), None).unwrap_err().to_string();
    assert!(err.contains(EXPANSION_LIMIT), "{err}");
}
