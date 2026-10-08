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

pub mod diffusers;
pub mod hf_keys;
pub mod tensors;

pub use diffusers::{DiffusersRead, DiffusersRoute, diffusers_class, is_diffusers, read_diffusers};
pub use tensors::{HeaderSource, TensorEntry, TensorIndex};

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
    /// A reader this crate ships as Rust, not data (a diffusers component's route: [`diffusers`]).
    CoreReader { id: String },
}

impl AdapterSource {
    /// One line for a human report.
    pub fn describe(&self) -> String {
        match self {
            AdapterSource::None => "none (Level A: the standard keys and tensor names)".into(),
            AdapterSource::BuiltIn { id, hash } => format!("built-in data file `{id}` ({})", &hash[..16.min(hash.len())]),
            AdapterSource::UserFile { id, hash } => format!("user-supplied data file `{id}` ({})", &hash[..16.min(hash.len())]),
            AdapterSource::CoreReader { id } => format!("built-in reader `{id}` (Rust, not a data file)"),
        }
    }
}

/// How to choose the adapter.
#[derive(Clone, Debug, Default)]
pub enum AdapterChoice {
    /// The built-in adapter that claims the configuration; else none (Level A).
    #[default]
    Auto,
    /// No adapter: Level A only.
    None,
    /// A built-in adapter by id.
    BuiltIn(String),
    /// A user-supplied adapter file's text.
    Text(String),
}

impl AdapterChoice {
    /// The command-line spelling: `none`, `builtin:<id>`, `auto`, or the path of an adapter file
    /// (`misaka.palw.model-adapter.v1`), read here.
    pub fn parse_arg(arg: &str) -> Result<AdapterChoice, LowerError> {
        match arg {
            "auto" => Ok(AdapterChoice::Auto),
            "none" => Ok(AdapterChoice::None),
            _ => match arg.strip_prefix("builtin:") {
                Some(id) => Ok(AdapterChoice::BuiltIn(id.to_string())),
                None => std::fs::read_to_string(arg)
                    .map(AdapterChoice::Text)
                    .map_err(|e| LowerError::Io(format!("adapter file {arg}: {e}"))),
            },
        }
    }
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
    /// The built-in refusal (`adapters/refusals.json`) a user-supplied adapter overrode, as
    /// `<architecture>: <why>` (FR-25): the architecture is refused by default, and this adapter passed the same
    /// validation as any other and reads it anyway. The report says so.
    pub overrides_refusal: Option<String>,
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
    read_model_with(config, tensors, opts, crate::quantfmt::QuantRegistry::builtin())
}

/// [`read_model`] with the quant formats of `reg`: a `quantization_config` the built-ins do not read is
/// read by the descriptors in it (`--quant-format`).
pub fn read_model_with(
    config: &Value,
    tensors: Option<&TensorIndex>,
    opts: &ReadOptions,
    reg: &crate::quantfmt::QuantRegistry,
) -> Result<ModelRead, ReadFailure> {
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
    // A refusal is the build's default word, not a proof of impossibility: an adapter the CALLER supplies may
    // override it (FR-25), provided it passes the same validation as every adapter — it can only emit a
    // `ModelSpec` from the existing vocabulary, so a feature the vocabulary lacks still ends in Level C — and the
    // report says so. An adapter chosen from the built-in pack never overrides one: a stale refusal is deleted
    // from the data in the commit that adds the adapter.
    let refusal = builtin::refusal_for(&arch);
    let user_adapter = matches!(opts.adapter, AdapterChoice::Text(_));
    if let Some((missing, why)) = &refusal
        && !user_adapter
    {
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
    let overridden = match (&refusal, user_adapter) {
        (Some((_, why)), true) => Some(format!("{arch}: {why}")),
        _ => None,
    };
    let model_type = root.get("model_type").and_then(Value::as_str);
    // Choose the route.
    enum Route {
        Adapter(Adapter),
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
            None => Route::Generic,
        },
    };
    // An adapter that only dispatches (a VLM wrapper: the text decoder's own `model_type` picks the
    // decoder adapter) is replaced by its target, chained up to four times.
    let route = match route {
        Route::Adapter(mut a) => {
            for _ in 0..4 {
                let Some(d) = a.value.get("dispatch").and_then(Value::as_object) else { break };
                let key = d.get("key").and_then(Value::as_str).unwrap_or("");
                let mut at: Option<&Value> = Some(config);
                for part in key.split('.') {
                    at = at.and_then(|v| v.get(part)).filter(|v| !v.is_null());
                }
                // The text decoder's `model_type`, else the adapter's default (a string, or an object
                // by wrapper architecture with `*` for the rest).
                let default = match d.get("default") {
                    Some(Value::String(s)) => Some(s.as_str()),
                    Some(Value::Object(m)) => m.get(&arch).or_else(|| m.get("*")).and_then(Value::as_str),
                    _ => None,
                };
                let want = at.and_then(Value::as_str).or(default).unwrap_or("").to_string();
                let target = d.get("to").and_then(Value::as_object).and_then(|m| m.get(&want)).and_then(Value::as_str);
                match target.and_then(builtin::by_id) {
                    Some(t) => a = t.clone(),
                    None => {
                        return Err(fail(
                            LowerError::not_lowerable(format!("{arch}: text decoder model_type `{want}` is not modelled")),
                            source_of(&a),
                        ));
                    }
                }
            }
            Route::Adapter(a)
        }
        other => other,
    };
    let mut read = match route {
        Route::Adapter(a) => read_with(&a, &arch, config, tensors, reg)?,
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
            let mut r = read_with(std, &arch, config, tensors, reg)?;
            r.adapter = AdapterSource::None;
            r
        }
    };
    read.overrides_refusal = overridden;
    Ok(read)
}

/// A successful read of an **encoder-decoder** (`ENCDEC_FROM_SPEC_V1`).
#[derive(Clone, Debug)]
pub struct EncDecRead {
    pub spec: crate::lower::encdec::EncDecSpec,
    pub adapter: AdapterSource,
    /// Config keys the class defaults supplied (the reader assumed them: confirm against the class).
    pub assumed_defaults: Vec<String>,
}

/// Whether a configuration is an encoder-decoder: the reader's own key (`is_encoder_decoder`), or an architecture
/// an adapter of kind `encdec` claims (an older hub config of T5 does not carry the key: the class default is true).
pub fn is_encoder_decoder(config: &Value) -> bool {
    if config.get("is_encoder_decoder").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    let arch = config.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("");
    builtin::find_encdec_for(arch, config.get("model_type").and_then(Value::as_str)).is_some()
}

/// Read an encoder-decoder configuration through an adapter of kind `encdec`: the built-in one that claims the
/// class, one by id, or one the caller supplies. There is no standard template for it (an encoder-decoder shares
/// its keys with no decoder), so `--adapter none` is a refusal. The decoder reader ([`read_model`]) refuses these
/// configurations; this is their reader, and the adapter is data like every other.
pub fn read_encdec(config: &Value, opts: &ReadOptions) -> Result<EncDecRead, ReadFailure> {
    let root = config.as_object().ok_or_else(|| fail(LowerError::bad("config.json is not an object"), AdapterSource::None))?;
    let arch = match root.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str) {
        Some(a) => a.to_string(),
        None => {
            return Err(fail(
                LowerError::not_lowerable("config has no `architectures`: the reference implementation cannot be identified (not a transformers config?)"),
                AdapterSource::None,
            ));
        }
    };
    let model_type = root.get("model_type").and_then(Value::as_str);
    let adapter: Adapter = match &opts.adapter {
        AdapterChoice::BuiltIn(id) => {
            builtin::by_id(id).cloned().ok_or_else(|| fail(LowerError::bad(format!("no built-in adapter `{id}`")), AdapterSource::None))?
        }
        AdapterChoice::Text(t) => adapter::parse(t, Origin::User).map_err(|e| fail(e, AdapterSource::None))?,
        AdapterChoice::None => {
            return Err(fail(
                LowerError::not_lowerable(format!("{arch}: an encoder–decoder has no standard template; it needs an adapter (kind `encdec`)")),
                AdapterSource::None,
            ));
        }
        AdapterChoice::Auto => match builtin::find_encdec_for(&arch, model_type) {
            Some(a) => a.clone(),
            None => {
                return Err(ReadFailure {
                    error: LowerError::not_lowerable(format!("`{arch}` is an encoder–decoder no adapter of kind `encdec` claims")),
                    adapter: AdapterSource::None,
                    unmapped_config_keys: Vec::new(),
                    missing: vec![MissingItem {
                        what: format!("an encoder–decoder adapter for `{arch}`"),
                        why: "no built-in adapter of kind `encdec` claims this architecture".into(),
                        general_primitive: None,
                    }],
                });
            }
        },
    };
    let src = source_of(&adapter);
    if adapter.kind() != "encdec" {
        return Err(fail(
            LowerError::not_lowerable(format!("{arch}: adapter `{}` is of kind `{}`, not `encdec`", adapter.id, adapter.kind())),
            src,
        ));
    }
    match eval::build_encdec_spec(&adapter, config) {
        Ok(b) => Ok(EncDecRead { spec: b.spec, adapter: src, assumed_defaults: b.assumed_defaults }),
        Err(e) => Err(fail(e, src)),
    }
}

/// A successful read of a **vision tower** (`VISION_FROM_SPEC_V1`).
#[derive(Clone, Debug)]
pub struct VisionRead {
    pub spec: crate::lower::vision::VisionSpec,
    pub adapter: AdapterSource,
    pub assumed_defaults: Vec<String>,
}

/// Whether an adapter of kind `vision` claims this configuration.
pub fn is_vision_tower(config: &Value) -> bool {
    let arch = config.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("");
    builtin::find_vision_for(arch, config.get("model_type").and_then(Value::as_str)).is_some()
}

/// Read a vision tower's configuration through an adapter of kind `vision` (the built-in one that claims it, one by id,
/// or the caller's). The class's input size and the processor's normalisation are not in `config.json`: the size is the
/// config's own `image_size` (via the adapter) and the normalisation the adapter's default.
pub fn read_vision(config: &Value, opts: &ReadOptions) -> Result<VisionRead, ReadFailure> {
    let root = config.as_object().ok_or_else(|| fail(LowerError::bad("config.json is not an object"), AdapterSource::None))?;
    let arch = root.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("").to_string();
    let model_type = root.get("model_type").and_then(Value::as_str);
    let adapter: Adapter = match &opts.adapter {
        AdapterChoice::BuiltIn(id) => builtin::by_id(id).cloned().ok_or_else(|| fail(LowerError::bad(format!("no built-in adapter `{id}`")), AdapterSource::None))?,
        AdapterChoice::Text(t) => adapter::parse(t, Origin::User).map_err(|e| fail(e, AdapterSource::None))?,
        AdapterChoice::None => {
            return Err(fail(LowerError::not_lowerable(format!("{arch}: a vision tower has no standard template; it needs an adapter (kind `vision`)")), AdapterSource::None));
        }
        AdapterChoice::Auto => match builtin::find_vision_for(&arch, model_type) {
            Some(a) => a.clone(),
            None => {
                return Err(fail(LowerError::not_lowerable(format!("`{arch}` is a vision tower no adapter of kind `vision` claims")), AdapterSource::None));
            }
        },
    };
    let src = source_of(&adapter);
    if adapter.kind() != "vision" {
        return Err(fail(LowerError::not_lowerable(format!("{arch}: adapter `{}` is of kind `{}`, not `vision`", adapter.id, adapter.kind())), src));
    }
    let built = eval::build_vision_spec(&adapter, config).map_err(|e| fail(e, src.clone()))?;
    let spec = crate::lower::vision::vision_spec_from_value(built.spec, None, None).map_err(|e| fail(e, src.clone()))?;
    Ok(VisionRead { spec, adapter: src, assumed_defaults: built.assumed_defaults })
}

/// A successful read of a **convolutional network** (`CNN_FROM_SPEC_V1`).
#[derive(Clone, Debug)]
pub struct CnnRead {
    pub spec: crate::lower::cnn::CnnSpec,
    pub adapter: AdapterSource,
    pub assumed_defaults: Vec<String>,
}

/// The image classes' canonical input size: a convolutional network's `config.json` declares none (the class does), so a read
/// without a class assumes the ImageNet convention and says so.
pub const CNN_DEFAULT_INPUT: (u32, u32) = (224, 224);

/// Whether an adapter of kind `cnn` claims this configuration.
pub fn is_cnn(config: &Value) -> bool {
    let arch = config.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("");
    builtin::find_cnn_for(arch, config.get("model_type").and_then(Value::as_str)).is_some()
}

/// Read a convolutional network's configuration through an adapter of kind `cnn` (the built-in one that claims it, one by id,
/// or the caller's). The class's input size and the processor's normalisation are not in `config.json`: the size is
/// [`CNN_DEFAULT_INPUT`] (an assumed default) and the normalisation the adapter's.
pub fn read_cnn(config: &Value, opts: &ReadOptions) -> Result<CnnRead, ReadFailure> {
    let root = config.as_object().ok_or_else(|| fail(LowerError::bad("config.json is not an object"), AdapterSource::None))?;
    let arch = root.get("architectures").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("").to_string();
    let model_type = root.get("model_type").and_then(Value::as_str);
    let adapter: Adapter = match &opts.adapter {
        AdapterChoice::BuiltIn(id) => builtin::by_id(id).cloned().ok_or_else(|| fail(LowerError::bad(format!("no built-in adapter `{id}`")), AdapterSource::None))?,
        AdapterChoice::Text(t) => adapter::parse(t, Origin::User).map_err(|e| fail(e, AdapterSource::None))?,
        AdapterChoice::None => {
            return Err(fail(LowerError::not_lowerable(format!("{arch}: a convolutional network has no standard template; it needs an adapter (kind `cnn`)")), AdapterSource::None));
        }
        AdapterChoice::Auto => match builtin::find_cnn_for(&arch, model_type) {
            Some(a) => a.clone(),
            None => {
                return Err(fail(LowerError::not_lowerable(format!("`{arch}` is a convolutional network no adapter of kind `cnn` claims")), AdapterSource::None));
            }
        },
    };
    let src = source_of(&adapter);
    if adapter.kind() != "cnn" {
        return Err(fail(LowerError::not_lowerable(format!("{arch}: adapter `{}` is of kind `{}`, not `cnn`", adapter.id, adapter.kind())), src));
    }
    let built = eval::build_cnn_spec(&adapter, config).map_err(|e| fail(e, src.clone()))?;
    let mut assumed_defaults = built.assumed_defaults;
    let given = built.spec.get("h").is_some_and(|v| !v.is_null()) && built.spec.get("w").is_some_and(|v| !v.is_null());
    let size = if given {
        None
    } else {
        assumed_defaults.push(format!("input size {}x{} (the class declares it; a convolutional network's config.json has none)", CNN_DEFAULT_INPUT.0, CNN_DEFAULT_INPUT.1));
        Some(CNN_DEFAULT_INPUT)
    };
    let spec = crate::lower::cnn::cnn_spec_from_value(built.spec, size, None).map_err(|e| fail(e, src.clone()))?;
    Ok(CnnRead { spec, adapter: src, assumed_defaults })
}

/// The reader's own checks (before any adapter runs), the adapter's evaluation, and the
/// pre-quantised-checkpoint attachment.
fn read_with(
    a: &Adapter,
    arch: &str,
    config: &Value,
    tensors: Option<&TensorIndex>,
    reg: &crate::quantfmt::QuantRegistry,
) -> Result<ModelRead, ReadFailure> {
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
            Some(crate::prequant::parse_quant_config_with(q, arch, mt, reg).map_err(f)?)
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
            // `REFERENCE_REMOTE_CODE_V1`: the file the adapter says its lowering follows, by hash (declared by the adapter, never attested).
            let pin = match a.value.get("remote_code_pin").and_then(|p| p.get(module.as_str())) {
                None | Some(Value::Null) => None,
                Some(Value::String(h)) if h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) => Some(h.clone()),
                Some(other) => {
                    return Err(f(LowerError::bad(format!(
                        "{arch}: remote_code_pin of `{module}` is a lowercase sha256 (64 hex characters), got {other}"
                    ))));
                }
            };
            Reference::RemoteCode { module, pin }
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
    Ok(ModelRead { spec, adapter: src, assumed_defaults: built.assumed_defaults, overrides_refusal: None })
}
