//! # The HF schema reader: Hugging Face config (and tensor names) → [`ModelSpec`]
//!
//! `model_type` does not select code. The reader turns a `config.json` — and, when it is given them,
//! the names and shapes of the checkpoint's tensors (a safetensors header is enough) — into a
//! feature-based [`ModelSpec`]:
//!
//! * **Level A — no adapter.** The configuration uses the standard keys and the standard tensor
//!   names; the reader's own inference (the standard decoder template, itself data) is all it takes.
//! * **Level B — an adapter.** The configuration uses keys or conventions of its own. An adapter is
//!   a *data file* (`misaka.palw.model-adapter.v1`, [`crate::adapter`]): config-key mappings, class
//!   defaults, weight-name patterns, feature selections and their parameters — a built-in one shipped
//!   with this crate, or one the caller supplies. No Rust code is involved: the features it selects
//!   are lowered by the generic lowerers.
//! * **Level C — a capability is missing.** A feature the model needs is not in the vocabulary or
//!   cannot be lowered. The reader says exactly which, and — for a protocol gap — the smallest
//!   *general* primitive that would close it.
//!
//! [`read_model`] is the entry point; [`crate::model::analyze`] wraps it in the report
//! `palw-class check-architecture` prints.

pub mod tensors;

pub use tensors::{TensorEntry, TensorIndex};

use crate::adapter::{self, Adapter, Origin, builtin, eval};
use crate::error::LowerError;
use crate::model::feature_info;
use crate::spec::{ModelSpec, Reference};
use serde::Serialize;
use serde_json::Value;

/// Which adapter, if any, produced a spec.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdapterSource {
    /// Level A: no adapter file was needed.
    None,
    /// A data file shipped with this crate; `hash` is the BLAKE2b-512 of its canonical JSON.
    BuiltIn { id: String, hash: String },
    /// A data file the caller supplied.
    UserFile { id: String, hash: String },
    /// A parser still written in Rust (being converted to data files; reported as such).
    LegacyRust { parser: String },
}

impl AdapterSource {
    /// One line for a human report.
    pub fn describe(&self) -> String {
        match self {
            AdapterSource::None => "none (Level A: the standard keys and tensor names)".into(),
            AdapterSource::BuiltIn { id, hash } => format!("built-in data file `{id}` ({})", &hash[..16.min(hash.len())]),
            AdapterSource::UserFile { id, hash } => format!("user-supplied data file `{id}` ({})", &hash[..16.min(hash.len())]),
            AdapterSource::LegacyRust { parser } => format!("legacy Rust parser `{parser}` (not yet a data file)"),
        }
    }
}

/// How to choose the adapter.
#[derive(Clone, Debug, Default)]
pub enum AdapterChoice {
    /// The built-in adapter that claims the configuration; else the legacy parsers while they last;
    /// else none (Level A).
    #[default]
    Auto,
    /// No adapter: Level A only.
    None,
    /// A built-in adapter by id.
    BuiltIn(String),
    /// A user-supplied adapter file's text.
    Text(String),
}

#[derive(Clone, Debug, Default)]
pub struct ReadOptions {
    pub adapter: AdapterChoice,
}

/// The support level of a model (RFC-0002 generic frontend).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Level {
    /// Automatic: the standard keys and tensor names suffice.
    A,
    /// A thin adapter (a data file): config → ModelSpec only, no protocol change.
    B,
    /// A capability is missing: a feature that cannot be lowered, or a protocol gap.
    C,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Level::A => "A",
            Level::B => "B",
            Level::C => "C",
        })
    }
}

/// A feature or capability the model needs and the build lacks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MissingItem {
    /// A [`crate::model::FeatureId`] when the vocabulary names it, else a description.
    pub what: String,
    pub why: String,
    /// For a protocol gap: the smallest general primitive that would close it.
    pub general_primitive: Option<String>,
}

/// A successful read.
#[derive(Clone, Debug)]
pub struct ModelRead {
    pub spec: ModelSpec,
    pub adapter: AdapterSource,
    /// Config keys the class defaults supplied (the reader assumed them: confirm against the class).
    pub assumed_defaults: Vec<String>,
}

/// A refused read: what was missing or unmapped.
#[derive(Clone, Debug)]
pub struct ReadFailure {
    pub error: LowerError,
    pub adapter: AdapterSource,
    /// Config keys no rule accounts for (they might change the math, so they are refused).
    pub unmapped_config_keys: Vec<String>,
    pub missing: Vec<MissingItem>,
}

impl std::fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for ReadFailure {}

/// Keys named by a "config key(s) this lowerer does not model: a, b — …" refusal.
fn keys_of_refusal(msg: &str) -> Vec<String> {
    const MARK: &str = "config key(s) this lowerer does not model: ";
    match msg.find(MARK) {
        Some(i) => {
            let rest = &msg[i + MARK.len()..];
            let end = rest.find(" — ").unwrap_or(rest.len());
            rest[..end].split(", ").map(str::to_string).filter(|k| !k.is_empty()).collect()
        }
        None => Vec::new(),
    }
}

fn fail(error: LowerError, adapter: AdapterSource) -> ReadFailure {
    let unmapped_config_keys = match &error {
        LowerError::NotLowerable(m) => keys_of_refusal(m),
        _ => Vec::new(),
    };
    ReadFailure { error, adapter, unmapped_config_keys, missing: Vec::new() }
}

/// Keys every configuration may carry that the reader itself accounts for (it checks them before any
/// adapter runs): remote-code wiring, the quantisation block, the cross-attention flags and another
/// library's settings.
pub const READER_KEYS: &[&str] =
    &["auto_map", "quantization_config", "is_encoder_decoder", "add_cross_attention", "transformers.js_config"];

fn source_of(a: &Adapter) -> AdapterSource {
    match a.origin {
        Origin::BuiltIn => AdapterSource::BuiltIn { id: a.id.clone(), hash: a.hash.clone() },
        Origin::User => AdapterSource::UserFile { id: a.id.clone(), hash: a.hash.clone() },
    }
}

/// Read a Hugging Face `config.json` (and, optionally, the checkpoint's tensor names and shapes)
/// into a [`ModelSpec`].
pub fn read_model(config: &Value, tensors: Option<&TensorIndex>, opts: &ReadOptions) -> Result<ModelRead, ReadFailure> {
    let root = config.as_object().ok_or_else(|| fail(LowerError::bad("config.json is not an object"), AdapterSource::None))?;
    let arch = match root.get("architectures") {
        Some(Value::Array(a)) if !a.is_empty() => match a[0].as_str() {
            Some(s) => s.to_string(),
            None => return Err(fail(LowerError::bad("architectures[0] is not a string"), AdapterSource::None)),
        },
        _ => {
            return Err(fail(
                LowerError::not_lowerable("config has no `architectures`: the reference implementation cannot be identified (not a transformers config?)"),
                AdapterSource::None,
            ));
        }
    };
    // An architecture refused on purpose, by the features it would need (data: `adapters/refusals.json`).
    if let Some((missing, why)) = builtin::refusal_for(&arch) {
        let items = missing
            .iter()
            .map(|id| MissingItem {
                what: id.clone(),
                why: feature_info(id).map(|f| f.title.to_string()).unwrap_or_else(|| why.clone()),
                general_primitive: None,
            })
            .collect();
        return Err(ReadFailure {
            error: LowerError::not_lowerable(format!("{arch}: {why}")),
            adapter: AdapterSource::None,
            unmapped_config_keys: Vec::new(),
            missing: items,
        });
    }
    let model_type = root.get("model_type").and_then(Value::as_str);
    // Choose the route.
    enum Route {
        Adapter(Adapter),
        Legacy,
        Generic,
    }
    let route = match &opts.adapter {
        AdapterChoice::BuiltIn(id) => Route::Adapter(
            builtin::by_id(id).cloned().ok_or_else(|| fail(LowerError::bad(format!("no built-in adapter `{id}`")), AdapterSource::None))?,
        ),
        AdapterChoice::Text(t) => Route::Adapter(adapter::parse(t, Origin::User).map_err(|e| fail(e, AdapterSource::None))?),
        AdapterChoice::None => Route::Generic,
        AdapterChoice::Auto => match builtin::find_for(&arch, model_type) {
            Some(a) => Route::Adapter(a.clone()),
            None if crate::hf_config::knows(&arch) => Route::Legacy,
            None => Route::Generic,
        },
    };
    match route {
        Route::Legacy => match crate::hf_config::parse_legacy(config) {
            Ok(spec) => Ok(ModelRead { spec, adapter: AdapterSource::LegacyRust { parser: arch }, assumed_defaults: Vec::new() }),
            Err(e) => Err(fail(e, AdapterSource::LegacyRust { parser: arch })),
        },
        Route::Adapter(a) => read_with(&a, &arch, config, tensors),
        Route::Generic => {
            // The standard decoder template is the reading of a causal language model whose class
            // follows the Hugging Face convention (`…ForCausalLM`): a configuration of another kind
            // (an encoder, an encoder–decoder, a vision tower) can share every standard key with a
            // decoder and still compute something else.
            if !arch.ends_with("ForCausalLM") && !matches!(opts.adapter, AdapterChoice::None) {
                return Err(ReadFailure {
                    error: LowerError::not_lowerable(format!(
                        "`{arch}` has no adapter and is not a causal language model class (`…ForCausalLM`): the standard decoder template is not applied to it \
                         (encoder-only, encoder–decoder, vision/audio, or a decoder not modelled yet)"
                    )),
                    adapter: AdapterSource::None,
                    unmapped_config_keys: Vec::new(),
                    missing: vec![MissingItem {
                        what: format!("an adapter for `{arch}`"),
                        why: "no built-in adapter claims this architecture and it is not a `…ForCausalLM` class".into(),
                        general_primitive: None,
                    }],
                });
            }
            let std = builtin::by_id("standard-decoder").ok_or_else(|| fail(LowerError::eval("internal: no standard-decoder adapter"), AdapterSource::None))?;
            let mut r = read_with(std, &arch, config, tensors)?;
            r.adapter = AdapterSource::None;
            Ok(r)
        }
    }
}

/// The reader's own checks (before any adapter runs), the adapter's evaluation, and the
/// pre-quantised-checkpoint attachment.
fn read_with(a: &Adapter, arch: &str, config: &Value, tensors: Option<&TensorIndex>) -> Result<ModelRead, ReadFailure> {
    let src = source_of(a);
    let f = |e: LowerError| fail(e, src.clone());
    let root = config.as_object().ok_or_else(|| f(LowerError::bad("config.json is not an object")))?;
    if root.get("is_encoder_decoder").and_then(Value::as_bool) == Some(true) {
        return Err(f(LowerError::not_lowerable(format!("{arch}: encoder–decoder models are out of scope for v1"))));
    }
    if root.get("add_cross_attention").and_then(Value::as_bool) == Some(true) {
        return Err(f(LowerError::not_lowerable(format!("{arch}: cross-attention is out of scope for v1"))));
    }
    // A pre-quantised checkpoint (GPTQ, AWQ): its integers are lowered as stored; every other
    // method is refused there.
    let quant = match root.get("quantization_config").filter(|q| !q.is_null()) {
        Some(q) => {
            let mt = root.get("model_type").and_then(Value::as_str).unwrap_or("");
            Some(crate::prequant::parse_quant_config(q, arch, mt).map_err(f)?)
        }
        None => None,
    };
    // Remote code: only modules the adapter models.
    let reference = match root.get("auto_map").and_then(Value::as_object) {
        Some(m) => {
            let module = crate::hf_config::remote_module(m).unwrap_or_default();
            if !a.remote_code().contains(&module.as_str()) {
                return Err(f(LowerError::not_lowerable(format!(
                    "{arch}: trust_remote_code module `{module}` is not modelled — its forward may differ from any transformers class"
                ))));
            }
            Reference::RemoteCode { module }
        }
        None => {
            let required = match a.value.get("remote_code_required") {
                Some(Value::Bool(b)) => *b,
                Some(Value::Array(l)) => l.iter().any(|x| x.as_str() == Some(arch)),
                _ => false,
            };
            if required {
                return Err(f(LowerError::not_lowerable(format!("{arch}: exists only as remote code and the config has no auto_map"))));
            }
            Reference::Native
        }
    };
    let built = eval::build_spec(a, config, tensors).map_err(f)?;
    let mut spec = built.spec;
    spec.reference = reference;
    if let Some(q) = quant {
        crate::hf_config::attach_quant(&mut spec, q).map_err(|e| fail(e, src.clone()))?;
    }
    Ok(ModelRead { spec, adapter: src, assumed_defaults: built.assumed_defaults })
}
