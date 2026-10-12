//! # Model adapters are data
//!
//! A *Level B* model — one whose configuration uses keys, defaults or tensor names of its own — is
//! described by an **adapter file**, JSON in the `misaka.palw.model-adapter.v1` format, loaded at run
//! time by the generic reader. Nothing in an adapter is Rust: it maps configuration keys onto the
//! features of [`crate::model`] (class defaults, weight-name patterns, feature selections and their
//! parameters) in the small expression language of [`expr`]. A new model is therefore a new data file
//! — written by anyone, pinned by its hash — never a new parser, primitive, runtime or court kernel.
//! Where a family cannot be written as a mapping, the gap is a missing *feature*, added once and
//! generically.
//!
//! ```jsonc
//! {
//!   "format": "misaka.palw.model-adapter.v1",
//!   "id": "my-model",
//!   "extends": ["standard-decoder"],       // built-in adapters merged first (later overrides earlier)
//!   "match":  { "architectures": ["MyForCausalLM"], "model_types": ["my_model"],
//!               "tower_of": [] },           // (kind vision) wrapper architectures whose tower this adapter reads
//!   "config": {
//!     "decoder": "text_config",            // the decoder's config lives in this nested object (VLMs)
//!     "scope": "vision_config",            // (kinds vision, cnn) the component's config lives in this nested object
//!     "defaults": { "rms_norm_eps": 1e-5 },// the HF config class's defaults
//!     "inert": ["some_training_only_key"], // keys known not to change the forward pass
//!     "root_inert": []                     // the same, for the wrapper's own keys
//!   },
//!   "vars":  [ { "name": "heads", "value": { "$cfg": "num_attention_heads" } } ],
//!   "spec":  { /* a ModelSpec whose values may be operator nodes */ },
//!   "refuse": [ { "missing": ["ATTN_CROSS_V1"], "why": "…" } ]   // a refusal, by named feature
//! }
//! ```
//!
//! **Merging** (`extends`): objects merge key by key, later wins; arrays and scalars are replaced (the `inert` and `root_inert` lists accumulate);
//! `{"$unset": true}` deletes a key; `vars` merge by `name` (a redefinition replaces in place, a new
//! name is appended). **Identity**: the BLAKE2b-512 of the canonical JSON (sorted keys, compact,
//! integral floats as integers) of the *effective* adapter — after `extends` — keyed
//! `misaka-palw/model-adapter/v1`. Lane F pins the hashes of the built-in pack.

pub mod builtin;
pub mod eval;
pub mod expr;

use crate::error::{LowerError, Result};
use serde_json::{Map, Value};

pub const ADAPTER_FORMAT_V1: &str = "misaka.palw.model-adapter.v1";
/// Key of the adapter hash (BLAKE2b-512, keyed).
pub const ADAPTER_HASH_KEY_V1: &[u8] = b"misaka-palw/model-adapter/v1";
/// An adapter file larger than this is refused.
pub const MAX_ADAPTER_BYTES: usize = 1 << 20;

/// Where an adapter came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
    User,
}

/// A parsed adapter: the effective JSON (after `extends`), its id and its hash.
#[derive(Clone, Debug)]
pub struct Adapter {
    pub id: String,
    pub origin: Origin,
    /// The effective adapter (every `extends` merged in).
    pub value: Value,
    /// Hex BLAKE2b-512 of [`canonical_json`] of `value`.
    pub hash: String,
}

fn bad(msg: impl Into<String>) -> LowerError {
    LowerError::bad(msg)
}

/// Canonical JSON: keys sorted, no whitespace, integral floats written as integers.
pub fn canonical_json(v: &Value) -> String {
    fn go(v: &Value, out: &mut String) {
        match v {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => {
                if let Some(f) = n.as_f64().filter(|_| n.is_f64())
                    && f.fract() == 0.0
                    && f.abs() < 9.0e15
                {
                    out.push_str(&(f as i64).to_string());
                } else {
                    out.push_str(&n.to_string());
                }
            }
            Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())),
            Value::Array(a) => {
                out.push('[');
                for (i, e) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    go(e, out);
                }
                out.push(']');
            }
            Value::Object(o) => {
                let mut keys: Vec<&String> = o.keys().collect();
                keys.sort();
                out.push('{');
                for (i, k) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(k).unwrap_or_else(|_| "\"\"".into()));
                    out.push(':');
                    go(&o[k], out);
                }
                out.push('}');
            }
        }
    }
    let mut s = String::new();
    go(v, &mut s);
    s
}

/// The hash of an (effective) adapter value.
pub fn hash_value(v: &Value) -> String {
    let h = blake2b_simd::Params::new().hash_length(64).key(ADAPTER_HASH_KEY_V1).hash(canonical_json(v).as_bytes());
    h.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn is_unset(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.len() == 1 && o.get("$unset") == Some(&Value::Bool(true)))
}

/// `vars` merge by name.
fn merge_vars(base: &[Value], over: &[Value]) -> Vec<Value> {
    let name = |v: &Value| v.get("name").and_then(Value::as_str).map(str::to_string);
    let mut out: Vec<Value> = base.to_vec();
    for o in over {
        match name(o).and_then(|n| out.iter().position(|b| name(b).as_deref() == Some(n.as_str()))) {
            Some(i) => out[i] = o.clone(),
            None => out.push(o.clone()),
        }
    }
    out
}

/// Merge `over` onto `base` (see the module docs).
pub fn merge(base: Value, over: Value) -> Value {
    match (base, over) {
        (Value::Object(mut b), Value::Object(o)) => {
            for (k, ov) in o {
                if is_unset(&ov) {
                    b.remove(&k);
                    continue;
                }
                let merged = match b.remove(&k) {
                    Some(Value::Array(ba)) if k == "vars" && ov.is_array() => {
                        Value::Array(merge_vars(&ba, ov.as_array().map(Vec::as_slice).unwrap_or(&[])))
                    }
                    // Lists of keys known to be inert accumulate.
                    Some(Value::Array(mut ba)) if (k == "inert" || k == "root_inert") && ov.is_array() => {
                        for e in ov.as_array().map(Vec::as_slice).unwrap_or(&[]) {
                            if !ba.contains(e) {
                                ba.push(e.clone());
                            }
                        }
                        Value::Array(ba)
                    }
                    Some(bv) => merge(bv, ov),
                    None => ov,
                };
                b.insert(k, merged);
            }
            Value::Object(b)
        }
        (_, over) => over,
    }
}

/// Parse an adapter file's text, resolving `extends` against the built-in adapters.
pub fn parse(text: &str, origin: Origin) -> Result<Adapter> {
    if text.len() > MAX_ADAPTER_BYTES {
        return Err(bad(format!("adapter file of {} bytes exceeds {MAX_ADAPTER_BYTES}", text.len())));
    }
    let v: Value = serde_json::from_str(text).map_err(|e| bad(format!("adapter is not JSON: {e}")))?;
    let value = resolve(v, 0)?;
    let id = value.get("id").and_then(Value::as_str).ok_or_else(|| bad("adapter has no `id`"))?.to_string();
    if id.is_empty() || id.len() > 128 {
        return Err(bad("adapter `id` is 1..=128 characters"));
    }
    let hash = hash_value(&value);
    Ok(Adapter { id, origin, value, hash })
}

fn resolve(v: Value, depth: usize) -> Result<Value> {
    if depth > 8 {
        return Err(bad("adapter `extends` nests too deeply (a cycle?)"));
    }
    let o = v.as_object().ok_or_else(|| bad("adapter is not an object"))?;
    if o.get("format").and_then(Value::as_str) != Some(ADAPTER_FORMAT_V1) {
        return Err(bad(format!("adapter `format` must be \"{ADAPTER_FORMAT_V1}\"")));
    }
    let mut effective = Value::Object(Map::new());
    if let Some(ext) = o.get("extends") {
        let ids: Vec<&str> = match ext {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => return Err(bad("`extends` is an id or a list of ids")),
        };
        for id in ids {
            let text = builtin::source(id).ok_or_else(|| bad(format!("adapter extends `{id}`, which is not a built-in adapter")))?;
            let base: Value = serde_json::from_str(text).map_err(|e| bad(format!("built-in adapter `{id}`: {e}")))?;
            effective = merge(effective, resolve(base, depth + 1)?);
        }
    }
    let mut own = v.clone();
    if let Some(m) = own.as_object_mut() {
        m.remove("extends");
    }
    Ok(merge(effective, own))
}

impl Adapter {
    /// What the adapter's `spec` instantiates: `"decoder"` (a [`crate::spec::ModelSpec`], the default) or
    /// `"encdec"` (a [`crate::lower::encdec::EncDecSpec`], `ENCDEC_FROM_SPEC_V1`) or `"vision"` (a
    /// [`crate::lower::vision::VisionSpec`], `VISION_FROM_SPEC_V1`) or `"cnn"` (a [`crate::lower::cnn::CnnSpec`],
    /// `CNN_FROM_SPEC_V1`).
    pub fn kind(&self) -> &str {
        self.value.get("kind").and_then(Value::as_str).unwrap_or("decoder")
    }

    pub fn architectures(&self) -> Vec<&str> {
        self.value
            .pointer("/match/architectures")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    pub fn model_types(&self) -> Vec<&str> {
        self.value.pointer("/match/model_types").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
    }

    /// The architectures of WRAPPER models whose component this adapter reads (`match.tower_of`: a VLM's vision tower). They are
    /// not `match.architectures`: a wrapper is still a text model for the reader, the component is reached by name.
    pub fn tower_of(&self) -> Vec<&str> {
        self.value.pointer("/match/tower_of").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
    }

    /// The architectures whose MASKED-LM reading this adapter is (`match.masked_lm_of`: `BertForMaskedLM` → `bert-mlm`). They are
    /// not `match.architectures`: a `…ForMaskedLM` checkpoint is also read as its encoder (a sentence embedder such as MPNet's
    /// `all-mpnet-base-v2`), so the head's reading is chosen by the TASK (`fill-mask`), never by the architecture alone.
    pub fn masked_lm_of(&self) -> Vec<&str> {
        self.value.pointer("/match/masked_lm_of").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
    }

    /// Remote-code modules (`auto_map` targets) this adapter models.
    pub fn remote_code(&self) -> Vec<&str> {
        self.value.get("remote_code").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
    }
}
