//! **Adapter + configuration → [`ModelSpec`]**: evaluate the adapter's variables and `spec`
//! template against the configuration (and, when given, the tensor names).

use super::{
    Adapter,
    expr::{Env, normalize},
};
use crate::cfg::Cfg;
use crate::error::{LowerError, Result};
use crate::hf_schema::TensorIndex;
use crate::spec::ModelSpec;
use serde_json::{Map, Value};

/// A spec built from an adapter.
#[derive(Clone, Debug)]
pub struct Built {
    pub spec: ModelSpec,
    /// Config keys whose value is the class default, not the checkpoint's.
    pub assumed_defaults: Vec<String>,
}

fn bad(msg: impl Into<String>) -> LowerError {
    LowerError::bad(msg)
}

/// Why an adapter refuses: the features it names as missing.
pub fn refusal_of(adapter: &Adapter) -> Option<Vec<(Vec<String>, String)>> {
    let r = adapter.value.get("refuse")?.as_array()?;
    Some(
        r.iter()
            .map(|e| {
                let missing = e
                    .get("missing")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default();
                (missing, e.get("why").and_then(Value::as_str).unwrap_or("").to_string())
            })
            .collect(),
    )
}

/// Instantiate `adapter` for `config`.
pub fn build_spec(adapter: &Adapter, config: &Value, tensors: Option<&TensorIndex>) -> Result<Built> {
    let root = config.as_object().ok_or_else(|| bad("config.json is not an object"))?;
    let arch =
        root.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("").to_string();
    let acfg = adapter.value.get("config").and_then(Value::as_object).cloned().unwrap_or_default();
    let decoder = acfg.get("decoder").and_then(Value::as_str);
    // `decoder_optional`: a wrapper whose older configurations are FLAT (Qwen2-VL before transformers 4.52: the decoder's keys at
    // the root, no `text_config`) is read from the root, its wrapper keys (`root_inert`) inert there.
    let optional = acfg.get("decoder_optional").and_then(Value::as_bool) == Some(true);
    let (dec_map, dec_path, flat): (&Map<String, Value>, &str, bool) = match decoder {
        Some(k) => match root.get(k).and_then(Value::as_object) {
            Some(m) => (m, k, false),
            None if optional => (root, "", true),
            None => return Err(bad(format!("{arch}: no `{k}` in the configuration"))),
        },
        None => (root, "", false),
    };
    let cfg = Cfg::new(arch.clone(), dec_map, dec_path);
    // Flat: `$root` reads the same object the decoder reads (every key's verdict is the decoder scope's).
    let root_cfg = decoder.map(|_| Cfg::new(arch.clone(), root, ""));
    if flat && let Some(r) = &root_cfg {
        r.inert(&root.keys().map(String::as_str).collect::<Vec<_>>());
    }
    let strs = |k: &str| -> Vec<String> {
        acfg.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let inert = strs("inert");
    cfg.inert(crate::hf_schema::READER_KEYS);
    cfg.inert(&inert.iter().map(String::as_str).collect::<Vec<_>>());
    if flat {
        let ri = strs("root_inert");
        cfg.inert(&ri.iter().map(String::as_str).collect::<Vec<_>>());
    }
    if let Some(r) = &root_cfg {
        r.inert(crate::hf_schema::READER_KEYS);
        let ri = strs("root_inert");
        r.inert(&ri.iter().map(String::as_str).collect::<Vec<_>>());
        if let Some(k) = decoder {
            r.inert(&[k]);
        }
        // A wrapper that mirrors its decoder's keys at the root (transformers >= 4.52 writes both): a root key EQUAL to the
        // decoder's is the decoder's, already read there; a root key that differs stays unread (refused).
        // `root_shadows_decoder` (a wrapper whose model reads only its decoder's config — transformers' VLM configs keep the old
        // flat keys at the root for compatibility, unused): every root key that names one of the decoder's keys is inert, whatever
        // its value; the decoder's own value is the one read.
        let shadows = acfg.get("root_shadows_decoder").and_then(Value::as_bool) == Some(true);
        let mirrored: Vec<&str> = root
            .iter()
            .filter(|(k, v)| dec_map.get(k.as_str()).is_some_and(|d| shadows || d == *v))
            .map(|(k, _)| k.as_str())
            .collect();
        r.inert(&mirrored);
    }
    let empty = Map::new();
    let defaults = acfg.get("defaults").and_then(Value::as_object).unwrap_or(&empty);
    let env = Env::new(&cfg, root_cfg.as_ref(), defaults, tensors, &arch);
    env.set_var("arch", Value::String(arch.clone()));
    if let Some(vars) = adapter.value.get("vars").and_then(Value::as_array) {
        let mut order = Vec::new();
        for v in vars {
            let name = v.get("name").and_then(Value::as_str).ok_or_else(|| bad("a variable has no `name`"))?;
            let expr = v.get("value").ok_or_else(|| bad(format!("variable `{name}` has no `value`")))?;
            // Every variable is evaluated (its checks and key reads happen) unless it says `lazy`:
            // then it is evaluated only where something reads it.
            let force = v.get("lazy").and_then(Value::as_bool) != Some(true);
            if v.get("layer").and_then(Value::as_bool) == Some(true) {
                env.add_layer_var(name, expr.clone(), force);
            } else {
                env.define_global(name, expr.clone());
                if force {
                    order.push(name.to_string());
                }
            }
        }
        env.force_globals(&order)?;
    }
    let template = adapter.value.get("spec").ok_or_else(|| bad(format!("adapter `{}` has no `spec`", adapter.id)))?;
    let mut out = normalize(env.eval(template)?);
    // `{p}` in the weight names is the tensor-name prefix the adapter chose (variable `p`).
    if let Ok(Value::String(p)) = env.eval(&serde_json::json!({"$var": "p"})) {
        subst_prefix(&mut out, &p);
    }
    let mut spec: ModelSpec =
        serde_json::from_value(out).map_err(|e| bad(format!("adapter `{}` produced an invalid ModelSpec: {e}", adapter.id)))?;
    if spec.architecture.is_empty() {
        spec.architecture = arch.clone();
    }
    cfg.finish()?;
    if let Some(r) = &root_cfg {
        r.finish()?;
    }
    let assumed_defaults = env.assumed.borrow().iter().cloned().collect();
    Ok(Built { spec, assumed_defaults })
}

/// An encoder-decoder spec built from an adapter of kind `encdec`.
#[derive(Clone, Debug)]
pub struct EncDecBuilt {
    pub spec: crate::lower::encdec::EncDecSpec,
    /// Config keys whose value is the class default, not the checkpoint's.
    pub assumed_defaults: Vec<String>,
}

/// Instantiate an adapter of kind `encdec` for `config`: the same evaluator as [`build_spec`] (variables, class
/// defaults, inert keys — and, as everywhere, a key nobody accounts for is refused), but the `spec` template
/// instantiates a [`crate::lower::encdec::EncDecSpec`] (`misaka.palw.encdec-spec.v1`), validated before anything is
/// sized on it. An encoder-decoder config is flat: there is no nested decoder section.
pub fn build_encdec_spec(adapter: &Adapter, config: &Value) -> Result<EncDecBuilt> {
    if adapter.kind() != "encdec" {
        return Err(bad(format!("adapter `{}` is of kind `{}`, not `encdec`", adapter.id, adapter.kind())));
    }
    let root = config.as_object().ok_or_else(|| bad("config.json is not an object"))?;
    let arch =
        root.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("").to_string();
    let acfg = adapter.value.get("config").and_then(Value::as_object).cloned().unwrap_or_default();
    let cfg = Cfg::new(arch.clone(), root, "");
    let strs = |k: &str| -> Vec<String> {
        acfg.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    cfg.inert(crate::hf_schema::READER_KEYS);
    cfg.inert(&strs("inert").iter().map(String::as_str).collect::<Vec<_>>());
    let empty = Map::new();
    let defaults = acfg.get("defaults").and_then(Value::as_object).unwrap_or(&empty);
    let env = Env::new(&cfg, None, defaults, None, &arch);
    env.set_var("arch", Value::String(arch.clone()));
    if let Some(vars) = adapter.value.get("vars").and_then(Value::as_array) {
        let mut order = Vec::new();
        for v in vars {
            let name = v.get("name").and_then(Value::as_str).ok_or_else(|| bad("a variable has no `name`"))?;
            let expr = v.get("value").ok_or_else(|| bad(format!("variable `{name}` has no `value`")))?;
            let force = v.get("lazy").and_then(Value::as_bool) != Some(true);
            env.define_global(name, expr.clone());
            if force {
                order.push(name.to_string());
            }
        }
        env.force_globals(&order)?;
    }
    let template = adapter.value.get("spec").ok_or_else(|| bad(format!("adapter `{}` has no `spec`", adapter.id)))?;
    let out = normalize(env.eval(template)?);
    let spec = crate::lower::encdec::encdec_spec_from_value(out).map_err(|e| {
        bad(format!("adapter `{}` produced an invalid {}: {e}", adapter.id, crate::lower::encdec::ENCDEC_SPEC_SCHEMA_V1))
    })?;
    cfg.finish()?;
    let assumed_defaults = env.assumed.borrow().iter().cloned().collect();
    Ok(EncDecBuilt { spec, assumed_defaults })
}

/// A spec value built from an adapter of a data kind (`vision`, `cnn`): the same evaluator as [`build_spec`] (variables, class
/// defaults, inert keys — and, as everywhere, a key nobody accounts for is refused) over the component's configuration, whose
/// `spec` template is the typed spec (not yet validated: the caller adds the class's input size and the processor's
/// normalisation).
///
/// **`config.scope`** names the nested object that holds the component's configuration when the model is a wrapper (a VLM's
/// `vision_config`); the template reads it with `$cfg` and the wrapper's own keys with `$root`. Every key of the nested object
/// must be read or declared inert; the wrapper's remaining keys are not this adapter's business (the wrapper's other adapter —
/// the text decoder's — accounts for them).
fn build_data_spec(adapter: &Adapter, config: &Value, kind: &str) -> Result<(Value, Vec<String>)> {
    if adapter.kind() != kind {
        return Err(bad(format!("adapter `{}` is of kind `{}`, not `{kind}`", adapter.id, adapter.kind())));
    }
    let root = config.as_object().ok_or_else(|| bad("config.json is not an object"))?;
    let arch =
        root.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("").to_string();
    let acfg = adapter.value.get("config").and_then(Value::as_object).cloned().unwrap_or_default();
    let scope = acfg.get("scope").and_then(Value::as_str);
    let (map, path): (&Map<String, Value>, &str) = match scope {
        Some(k) => (root.get(k).and_then(Value::as_object).ok_or_else(|| bad(format!("{arch}: no `{k}` in the configuration")))?, k),
        None => (root, ""),
    };
    let cfg = Cfg::new(arch.clone(), map, path);
    let root_cfg = scope.map(|_| Cfg::new(arch.clone(), root, ""));
    let strs = |k: &str| -> Vec<String> {
        acfg.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    cfg.inert(crate::hf_schema::READER_KEYS);
    cfg.inert(&strs("inert").iter().map(String::as_str).collect::<Vec<_>>());
    let empty = Map::new();
    let defaults = acfg.get("defaults").and_then(Value::as_object).unwrap_or(&empty);
    let env = Env::new(&cfg, root_cfg.as_ref(), defaults, None, &arch);
    env.set_var("arch", Value::String(arch.clone()));
    if let Some(vars) = adapter.value.get("vars").and_then(Value::as_array) {
        let mut order = Vec::new();
        for v in vars {
            let name = v.get("name").and_then(Value::as_str).ok_or_else(|| bad("a variable has no `name`"))?;
            let expr = v.get("value").ok_or_else(|| bad(format!("variable `{name}` has no `value`")))?;
            let force = v.get("lazy").and_then(Value::as_bool) != Some(true);
            env.define_global(name, expr.clone());
            if force {
                order.push(name.to_string());
            }
        }
        env.force_globals(&order)?;
    }
    let template = adapter.value.get("spec").ok_or_else(|| bad(format!("adapter `{}` has no `spec`", adapter.id)))?;
    let out = normalize(env.eval(template)?);
    cfg.finish()?;
    let assumed_defaults = env.assumed.borrow().iter().cloned().collect();
    Ok((out, assumed_defaults))
}

/// A vision-tower spec built from an adapter of kind `vision`.
#[derive(Clone, Debug)]
pub struct VisionBuilt {
    /// The instantiated `misaka.palw.vision-spec.v1` (not yet validated: the caller adds the class's input size and
    /// the processor's normalisation).
    pub spec: Value,
    pub assumed_defaults: Vec<String>,
}

/// Instantiate an adapter of kind `vision` for `config` (`VISION_FROM_SPEC_V1`): a `spec` template that is a
/// [`crate::lower::vision::VisionSpec`]. A wrapping model's tower config is reached with `$scope`.
pub fn build_vision_spec(adapter: &Adapter, config: &Value) -> Result<VisionBuilt> {
    let (spec, assumed_defaults) = build_data_spec(adapter, config, "vision")?;
    Ok(VisionBuilt { spec, assumed_defaults })
}

/// A convolutional-network spec built from an adapter of kind `cnn`.
#[derive(Clone, Debug)]
pub struct CnnBuilt {
    /// The instantiated `misaka.palw.cnn-spec.v1` (not yet validated: the caller adds the class's input size and the
    /// processor's normalisation).
    pub spec: Value,
    pub assumed_defaults: Vec<String>,
}

/// Instantiate an adapter of kind `cnn` for `config` (`CNN_FROM_SPEC_V1`): a `spec` template that is a
/// [`crate::lower::cnn::CnnSpec`].
pub fn build_cnn_spec(adapter: &Adapter, config: &Value) -> Result<CnnBuilt> {
    let (spec, assumed_defaults) = build_data_spec(adapter, config, "cnn")?;
    Ok(CnnBuilt { spec, assumed_defaults })
}

/// `{p}` in every string of a weight expression (`hf.weights`: the tensor names inside nested steps).
fn subst_all(v: &mut Value, p: &str) {
    match v {
        Value::String(s) => *s = s.replace("{p}", p),
        Value::Array(a) => a.iter_mut().for_each(|e| subst_all(e, p)),
        Value::Object(o) => o.values_mut().for_each(|e| subst_all(e, p)),
        _ => {}
    }
}

fn subst_prefix(v: &mut Value, p: &str) {
    if let Some(hf) = v.get_mut("hf") {
        if let Some(w) = hf.get_mut("weights") {
            subst_all(w, p);
        }
        if let Some(names) = hf.get_mut("names").and_then(Value::as_object_mut) {
            for n in names.values_mut() {
                if let Value::String(s) = n {
                    *s = s.replace("{p}", p);
                }
            }
        }
        if let Some(al) = hf.get_mut("prefix_aliases").and_then(Value::as_array_mut) {
            for pair in al {
                if let Some(a) = pair.as_array_mut() {
                    for e in a {
                        if let Value::String(s) = e {
                            *s = s.replace("{p}", p);
                        }
                    }
                }
            }
        }
        if let Some(ig) = hf.get_mut("ignored_prefixes").and_then(Value::as_array_mut) {
            for e in ig {
                if let Value::String(s) = e {
                    *s = s.replace("{p}", p);
                }
            }
        }
    }
}
