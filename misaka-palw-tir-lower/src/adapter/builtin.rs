//! **The built-in adapter pack**: data files shipped with this crate (`adapters/*.json`), parsed
//! once. The pack's hash — over the sorted `(id, hash)` pairs — is what the runtime pack pins.

use super::{Adapter, Origin, parse};
use std::sync::OnceLock;

macro_rules! pack {
    ($($id:literal),* $(,)?) => {
        /// `(id, file text)` of every built-in adapter.
        pub const FILES: &[(&str, &str)] = &[$(($id, include_str!(concat!("../../adapters/", $id, ".json")))),*];
    };
}

pack!(
    "refusals",
    "spec-frame",
    "encdec-frame",
    "mixin-bart-lineage",
    "t5",
    "t5-encoder",
    "longt5",
    "whisper",
    "bart",
    "mbart",
    "marian",
    "pegasus",
    "decoder-core",
    "standard-decoder",
    "mixin-gdn-hybrid",
    "mixin-gemma",
    "mixin-post-norm",
    "mixin-qwen-sliding",
    "mixin-vlm",
    "mixin-bert-encoder",
    "nomic-bert",
    "modernbert",
    "albert",
    "deberta-v2",
    "clip-vision",
    "siglip-vision",
    "vit",
    "qwen2-vl-vision",
    "qwen2-vl-vision-in-vlm",
    "qwen2-5-vl-vision",
    "qwen2-5-vl-vision-in-vlm",
    "qwen3-5-vision",
    "qwen3-5-vision-in-vlm",
    "llava-vision",
    "resnet",
    "convnext",
    "mobilenet-v2",
    "mobilenet-v1",
    "bitnet",
    "diffllama",
    "apertus",
    "lfm2",
    "kimi-linear",
    "zamba2",
    "chatglm3",
    "nemotron-h",
    "falcon-h1",
    "bert",
    "bloom",
    "clip-text",
    "cohere",
    "cohere2",
    "deepseek-v2",
    "dbrx",
    "deepseek-v3",
    "deepseek-v32",
    "longcat-flash",
    "jetmoe",
    "mllama",
    "deepseek-v4",
    "ernie4-5-moe",
    "exaone",
    "exaone4",
    "falcon",
    "falcon-mamba",
    "gemma",
    "gemma2",
    "gemma3-text",
    "gemma4-text",
    "gemma3n-text",
    "glm",
    "glm4",
    "glm4-moe",
    "gpt-bigcode",
    "gpt-neo",
    "gpt-neox",
    "gpt-oss",
    "gpt2",
    "gptj",
    "granite",
    "granitemoe",
    "granitemoehybrid",
    "distilbert",
    "internlm2",
    "jamba",
    "llama",
    "llama4-text",
    "mamba",
    "mamba2",
    "minicpm",
    "mistral",
    "mixtral",
    "mpnet",
    "mpt",
    "nemotron",
    "olmo",
    "olmo2",
    "olmo3",
    "olmoe",
    "opt",
    "phi",
    "phi3",
    "phimoe",
    "qwen2",
    "qwen2-moe",
    "qwen3",
    "qwen3-5",
    "qwen3-5-moe",
    "qwen3-moe",
    "qwen3-next",
    "qwen4-exp",
    "roberta",
    "rwkv4",
    "smollm3",
    "stablelm",
    "starcoder2",
    "vlm",
    "vlm-generic",
    "vlm-gemma3",
    "paligemma",
    "vlm-gemma",
    "vlm-gemma2",
    "vlm-llama",
    "vlm-llama4",
    "vlm-mistral",
    "vlm-qwen2",
    "vlm-qwen2-vl",
    "vlm-qwen3-5",
    "vlm-qwen3-5-moe",
    "mixin-seqcls",
    "llama-seqcls",
    "qwen2-seqcls",
    "qwen3-seqcls",
    "mistral-seqcls",
    "gemma-seqcls",
    "gemma2-seqcls",
    "phi3-seqcls",
    "mixtral-seqcls",
    "qwen3-moe-seqcls",
    "olmo2-seqcls",
    "gpt2-seqcls",
    "opt-seqcls",
    "mixin-seqcls-encoder",
    "bert-seqcls",
    "roberta-seqcls",
    "distilbert-seqcls",
);

/// The text of a built-in adapter, by id.
pub fn source(id: &str) -> Option<&'static str> {
    FILES.iter().find(|(i, _)| *i == id).map(|(_, t)| *t)
}

/// Every built-in adapter, parsed. A built-in that does not parse is a bug of this crate, caught by
/// the pack test; here it is skipped so one bad file cannot take the reader down.
pub fn all() -> &'static [Adapter] {
    static PACK: OnceLock<Vec<Adapter>> = OnceLock::new();
    PACK.get_or_init(|| FILES.iter().filter_map(|(_, t)| parse(t, Origin::BuiltIn).ok()).collect())
}

pub fn by_id(id: &str) -> Option<&'static Adapter> {
    all().iter().find(|a| a.id == id)
}

/// The built-in DECODER adapter claiming a configuration: by `architectures[0]`, else by `model_type`.
/// (An adapter of kind `encdec` is found by [`find_encdec_for`]; the decoder reader never sees one.)
pub fn find_for(arch: &str, model_type: Option<&str>) -> Option<&'static Adapter> {
    let real =
        || all().iter().filter(|a| a.kind() == "decoder" && (a.value.get("spec").is_some() || a.value.get("dispatch").is_some()));
    real()
        .find(|a| a.architectures().contains(&arch))
        .or_else(|| model_type.and_then(|m| real().find(|a| a.architectures().is_empty() && a.model_types().contains(&m))))
}

/// The built-in ENCODER-DECODER adapter (kind `encdec`) claiming a configuration, by `architectures[0]`, else by
/// `model_type`.
pub fn find_encdec_for(arch: &str, model_type: Option<&str>) -> Option<&'static Adapter> {
    let real = || all().iter().filter(|a| a.kind() == "encdec" && a.value.get("match").is_some());
    real()
        .find(|a| a.architectures().contains(&arch))
        .or_else(|| model_type.and_then(|m| real().find(|a| a.architectures().is_empty() && a.model_types().contains(&m))))
}

/// The built-in VISION-TOWER adapter (kind `vision`) claiming a configuration, by `architectures[0]`, else by `model_type`.
pub fn find_vision_for(arch: &str, model_type: Option<&str>) -> Option<&'static Adapter> {
    let real = || all().iter().filter(|a| a.kind() == "vision" && a.value.get("match").is_some());
    real()
        .find(|a| a.architectures().contains(&arch))
        .or_else(|| model_type.and_then(|m| real().find(|a| a.architectures().is_empty() && a.model_types().contains(&m))))
}

/// The built-in VISION-TOWER adapter (kind `vision`) that reads the tower INSIDE a wrapper model (`match.tower_of`: a VLM's
/// `Qwen2VLForConditionalGeneration`, `LlavaForConditionalGeneration`). The wrapper itself is a text model: only the
/// component's lowering asks for this.
pub fn find_tower_in(arch: &str) -> Option<&'static Adapter> {
    all().iter().filter(|a| a.kind() == "vision" && a.value.get("match").is_some()).find(|a| a.tower_of().contains(&arch))
}

/// The built-in CONVOLUTIONAL-NETWORK adapter (kind `cnn`) claiming a configuration, by `architectures[0]`, else by `model_type`.
pub fn find_cnn_for(arch: &str, model_type: Option<&str>) -> Option<&'static Adapter> {
    let real = || all().iter().filter(|a| a.kind() == "cnn" && a.value.get("match").is_some());
    real()
        .find(|a| a.architectures().contains(&arch))
        .or_else(|| model_type.and_then(|m| real().find(|a| a.architectures().is_empty() && a.model_types().contains(&m))))
}

/// **The generic text-decoder-in-a-wrapper route** (`vlm-generic`, data): a `…ForConditionalGeneration` wrapper no adapter names,
/// whose decoder lives in a nested `text_config` with its own `model_type`. `None` when the configuration is not such a wrapper
/// (the caller then keeps its other routes); `Some(Err)` names why a wrapper that IS one cannot be read (its text decoder's
/// `model_type` is claimed by no decoder adapter). `Some(Ok)` is the effective adapter: the decoder adapter the text `model_type`
/// selects — the `vlm` dispatch table's entry where it names one (family knowledge: Qwen3.5's `mtp.` head), else the decoder
/// adapter claiming that `model_type` over `mixin-vlm` — with `vlm-generic`'s overlay merged last (the prefix found in the index,
/// the head, the tie rule, the non-text components). Never keyed by the wrapper's own name.
pub fn find_wrapper_for(arch: &str, config: &serde_json::Value) -> Option<std::result::Result<Adapter, String>> {
    use serde_json::Value;
    let generic = by_id("vlm-generic")?;
    let rule = generic.value.pointer("/match/wrapper")?;
    let suffixes: Vec<&str> = rule.get("suffixes")?.as_array()?.iter().filter_map(Value::as_str).collect();
    if !suffixes.iter().any(|s| arch.ends_with(s) && arch.len() > s.len()) {
        return None;
    }
    let decoder_key = rule.get("decoder")?.as_str()?;
    let text = config.get(decoder_key)?.as_object()?;
    let mt = text.get("model_type").and_then(Value::as_str)?;
    let dispatched = by_id("vlm")
        .and_then(|v| v.value.pointer("/dispatch/to").and_then(Value::as_object).and_then(|m| m.get(mt)).and_then(Value::as_str))
        .and_then(by_id);
    let base = match dispatched {
        Some(a) => a.value.clone(),
        None => {
            let decoder = all().iter().find(|a| {
                a.kind() == "decoder"
                    && (a.value.get("spec").is_some() || a.value.get("dispatch").is_some())
                    && a.value.get("dispatch").is_none()
                    && a.model_types().contains(&mt)
            });
            let Some(decoder) = decoder else {
                return Some(Err(format!(
                    "{arch}: its text decoder (`{decoder_key}.model_type` = `{mt}`) is claimed by no decoder adapter"
                )));
            };
            let vlm = by_id("mixin-vlm")?;
            super::merge(decoder.value.clone(), vlm.value.clone())
        }
    };
    let mut overlay = generic.value.clone();
    if let Some(o) = overlay.as_object_mut() {
        o.remove("match");
    }
    let mut value = super::merge(base, overlay);
    let base_id = dispatched.map(|a| a.id.clone()).unwrap_or_else(|| format!("mixin-vlm+{mt}"));
    let id = format!("vlm-generic({base_id})");
    if let Some(o) = value.as_object_mut() {
        o.insert("id".into(), Value::String(id.clone()));
        o.remove("dispatch");
    }
    let hash = super::hash_value(&value);
    Some(Ok(Adapter { id, origin: Origin::BuiltIn, value, hash }))
}

/// A refusal on purpose: the features the architecture would need and the build lacks, and why.
pub fn refusal_for(arch: &str) -> Option<(Vec<String>, String)> {
    let a = by_id("refusals")?;
    a.value.get("refuse")?.as_array()?.iter().find_map(|e| {
        let archs = e.get("architectures")?.as_array()?;
        archs.iter().any(|x| x.as_str() == Some(arch)).then(|| {
            (
                e.get("missing")
                    .and_then(|m| m.as_array())
                    .map(|m| m.iter().filter_map(|x| x.as_str()).map(str::to_string).collect())
                    .unwrap_or_default(),
                e.get("why").and_then(|w| w.as_str()).unwrap_or("").to_string(),
            )
        })
    })
}

/// `(id, hash)` of the pack, sorted by id.
pub fn pack_manifest() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = all().iter().map(|a| (a.id.clone(), a.hash.clone())).collect();
    v.sort();
    v
}

/// The hash of the whole pack: BLAKE2b-512, keyed, over `id NUL hash LF` of each adapter in id order.
pub fn pack_hash() -> String {
    let mut st = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/model-adapter-pack/v1").to_state();
    for (id, h) in pack_manifest() {
        st.update(id.as_bytes());
        st.update(&[0]);
        st.update(h.as_bytes());
        st.update(b"\n");
    }
    st.finalize().as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
