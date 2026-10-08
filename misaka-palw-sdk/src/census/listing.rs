//! **One repository as the Hub's listing knows it** (RFC-0002 §II.10.2), and what follows from the listing alone: the declared task,
//! the selected artifact (one per repository, by a published rule), the strata, and the plan of what a header fetch must read.
//!
//! The record ([`ListingV1`]) is the compactor's (`tools/hf_census/compact.py`): the fields of one `/api/models` entry, renamed and
//! without the chat templates, plus the base references looked up in the same snapshot. Nothing here fetches anything.

use super::tasks::{Profile, TaskRow, task_row};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The listing record of one repository.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ListingV1 {
    pub id: String,
    /// The repository's ObjectId (creation order).
    pub oid: String,
    /// The commit the snapshot pins.
    pub sha: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub pipeline_tag: Option<String>,
    pub library: Option<String>,
    /// `false`, `"auto"` or `"manual"`.
    pub gated: serde_json::Value,
    pub disabled: bool,
    pub private: bool,
    pub downloads: u64,
    pub downloads_all: u64,
    pub likes: u64,
    pub tags: Vec<String>,
    pub license: Option<String>,
    pub license_name: Option<String>,
    pub license_link: Option<String>,
    /// The Hub's `baseModels`: the relation (`finetune`, `quantized`, `adapter`, `merge`) and the base ids.
    pub base_relation: Option<String>,
    pub base_ids: Vec<String>,
    /// The base ids looked up in the same snapshot.
    pub base_resolved: Vec<BaseResolvedV1>,
    /// The Hub's configuration summary (`architectures`, `model_type`, `auto_map`, `quantization_config`, `diffusers`, `peft`, …),
    /// without the tokenizer and processor templates.
    pub config: serde_json::Value,
    /// The Hub's `transformersInfo` (`auto_model`, `custom_class`, `pipeline_tag`, `processor`).
    pub tinfo: serde_json::Value,
    /// Parameters by safetensors dtype, as the Hub counts them from the headers.
    pub st_params: Option<serde_json::Value>,
    pub st_total: Option<u64>,
    /// The Hub's GGUF summary (`total`, `architecture`, `context_length`).
    pub gguf_total: Option<u64>,
    pub gguf_arch: Option<String>,
    pub gguf_ctx: Option<u64>,
    pub siblings: Vec<String>,
}

/// A base reference, looked up in the snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BaseResolvedV1 {
    pub id: String,
    /// The base is a repository of the snapshot.
    pub found: bool,
    pub sha: Option<String>,
    pub gated: bool,
    pub disabled: bool,
    pub license: Option<String>,
    /// Base resolution v2: the id the card or adapter config wrote, when the Hub redirects it to `id` (a renamed repository).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
}

/// **The base an adapter is composed with** (base resolution v2): the adapter's own `adapter_config.json`
/// `base_model_name_or_path` when it names one — what PEFT loads — else the card's single base; matched through a rename.
/// `Err((arg, evidence))` names why none is pinned: `absent`, `ambiguous` (several card bases and no adapter config naming one),
/// `base_gated`, `not_in_snapshot`.
pub fn pinned_base(l: &ListingV1) -> Result<&BaseResolvedV1, (&'static str, String)> {
    let peft = l.config.get("peft").and_then(|p| p.get("base_model_name_or_path")).and_then(|b| b.as_str()).filter(|b| !b.is_empty());
    let id = match (peft, l.base_ids.as_slice()) {
        (Some(b), _) => b.to_string(),
        (None, [one]) => one.clone(),
        (None, []) => return Err(("absent", "an adapter that names no base".into())),
        (None, many) => return Err(("ambiguous", format!("{} bases: {}", many.len(), many.join(", ")))),
    };
    let r = l.base_resolved.iter().find(|r| r.id == id || r.renamed_from.as_deref() == Some(id.as_str()));
    match r {
        Some(r) if r.found && !r.gated && !r.disabled && r.sha.is_some() => Ok(r),
        Some(r) if r.found && r.gated => Err(("base_gated", format!("base {} is gated", r.id))),
        _ => Err((
            "not_in_snapshot",
            format!("base `{id}` is not a public repository of the snapshot (a local path, a renamed or private repository)"),
        )),
    }
}

impl ListingV1 {
    pub fn gated(&self) -> Option<String> {
        match &self.gated {
            serde_json::Value::Bool(false) | serde_json::Value::Null => None,
            serde_json::Value::Bool(true) => Some("true".into()),
            serde_json::Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        }
    }

    pub fn arch(&self) -> Option<&str> {
        self.config.get("architectures").and_then(|a| a.get(0)).and_then(|a| a.as_str())
    }

    pub fn model_type(&self) -> Option<&str> {
        self.config.get("model_type").and_then(|a| a.as_str())
    }

    pub fn has_tag(&self, t: &str) -> bool {
        self.tags.iter().any(|x| x == t)
    }

    /// The parameter count the Hub reports (safetensors, else GGUF).
    pub fn params(&self) -> Option<u64> {
        self.st_total.or(self.gguf_total)
    }

    /// Remote code: the configuration names an `auto_map`, or the Hub tags it `custom_code`.
    pub fn remote_code(&self) -> bool {
        self.config.get("auto_map").is_some_and(|m| !m.is_null()) || self.has_tag("custom_code")
    }

    /// The quantisation the listing announces (`quantization_config.quant_method`), when it does.
    pub fn quant_method(&self) -> Option<String> {
        let q = self.config.get("quantization_config")?;
        if q.is_null() {
            return None;
        }
        Some(q.get("quant_method").and_then(|m| m.as_str()).unwrap_or("unnamed").to_ascii_lowercase())
    }
}

// ---- the task ---------------------------------------------------------------------------------------------------------------------

/// Where the task came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskV1 {
    pub task: String,
    pub group: String,
    pub profile: Profile,
    /// `pipeline_tag`, `transformers_info`, `inferred:architecture`, `inferred:peft`, or `none`.
    pub source: String,
}

/// The declared task: the card's `pipeline_tag`, else the Hub's `transformersInfo.pipeline_tag`, else — conservatively — a causal
/// language model's architecture name or a PEFT adapter's `CAUSAL_LM` task type (both say "text generation" in so many words). Nothing
/// else is inferred: an undeclared task is `TASK_UNKNOWN`.
pub fn task_of(l: &ListingV1) -> TaskV1 {
    let mk = |t: &str, src: &str| {
        let r: TaskRow = task_row(t);
        TaskV1 { task: t.to_string(), group: r.group.to_string(), profile: r.profile, source: src.to_string() }
    };
    if let Some(t) = l.pipeline_tag.as_deref().filter(|t| !t.is_empty()) {
        return mk(t, "pipeline_tag");
    }
    if let Some(t) = l.tinfo.get("pipeline_tag").and_then(|t| t.as_str()).filter(|t| !t.is_empty()) {
        return mk(t, "transformers_info");
    }
    if let Some(a) = l.arch()
        && (a.ends_with("ForCausalLM") || a.ends_with("LMHeadModel"))
    {
        return mk("text-generation", "inferred:architecture");
    }
    if let Some(t) = l.config.get("peft").and_then(|p| p.get("task_type")).and_then(|t| t.as_str()).and_then(task_of_peft_task_type) {
        return mk(t, "inferred:peft");
    }
    // Inference v2 (2026-10-04): what the files declare — the configuration's architecture class, the GGUF's `general.architecture` —
    // read against transformers' own head classes and llama.cpp's architecture names. Never the repository's code, name or card text.
    if let Some(t) = l.arch().and_then(task_of_architecture_class) {
        return mk(t, "inferred:architecture-class");
    }
    if let Some(t) = l.gguf_arch.as_deref().and_then(task_of_gguf_architecture) {
        return mk(t, "inferred:gguf-architecture");
    }
    TaskV1 { task: "unknown".into(), group: "unknown".into(), profile: Profile::None, source: "none".into() }
}

/// **The task a PEFT adapter declares** (`adapter_config.json`'s `task_type`, a closed enumeration of the PEFT library: the head the
/// adapter was trained with). `None` for a `task_type` this table does not list, and for an adapter that declares none (it then
/// carries its pinned base's task, [`super::store::base_task_of`]).
pub fn task_of_peft_task_type(t: &str) -> Option<&'static str> {
    Some(match t {
        "CAUSAL_LM" => "text-generation",
        "SEQ_2_SEQ_LM" => "text2text-generation",
        "SEQ_CLS" => "text-classification",
        "TOKEN_CLS" => "token-classification",
        "QUESTION_ANS" => "question-answering",
        "FEATURE_EXTRACTION" => "feature-extraction",
        _ => return None,
    })
}

/// **The task a transformers head class names** (the class transformers' pipelines resolve for that task); `None` for a class
/// that names none (a bare `…Model` other than a known sentence encoder, a custom class).
pub fn task_of_architecture_class(a: &str) -> Option<&'static str> {
    const SEQ2SEQ_TEXT: &[&str] = &[
        "T5",
        "MT5",
        "UMT5",
        "Bart",
        "MBart",
        "Marian",
        "Pegasus",
        "PegasusX",
        "M2M100",
        "NllbMoe",
        "LongT5",
        "Blenderbot",
        "BlenderbotSmall",
        "ProphetNet",
        "LED",
        "FSMT",
        "PLBart",
        "BigBirdPegasus",
    ];
    const SPEECH: &[&str] = &["Whisper", "Speech2Text", "SeamlessM4T", "SeamlessM4Tv2"];
    const VISION_CHAT: &[&str] = &[
        "Qwen2VL",
        "Qwen2_5_VL",
        "Qwen3VL",
        "Qwen3VLMoe",
        "Qwen3_5",
        "Qwen3_5Moe",
        "Qwen2_5Omni",
        "Gemma3",
        "Gemma3n",
        "Gemma4",
        "Llava",
        "LlavaNext",
        "LlavaOnevision",
        "LlavaNextVideo",
        "Idefics",
        "Idefics2",
        "Idefics3",
        "PaliGemma",
        "Mllama",
        "Mistral3",
        "SmolVLM",
        "InternVL",
        "Llama4",
        "AyaVision",
        "Florence2",
        "Blip2",
        "InstructBlip",
        "Kosmos2",
        "Chameleon",
        "Emu3",
        "Janus",
        "GotOcr2",
        "Glm4v",
        "Glm4vMoe",
    ];
    const ENCODERS: &[&str] = &[
        "Bert",
        "Roberta",
        "XLMRoberta",
        "DistilBert",
        "MPNet",
        "Electra",
        "DebertaV2",
        "Deberta",
        "ModernBert",
        "Camembert",
        "Albert",
        "NomicBert",
    ];
    let suffixes: &[(&str, &str)] = &[
        ("ForCausalLM", "text-generation"),
        ("LMHeadModel", "text-generation"),
        ("ForSequenceClassification", "text-classification"),
        ("ForTokenClassification", "token-classification"),
        ("ForQuestionAnswering", "question-answering"),
        ("ForMaskedLM", "fill-mask"),
        ("ForImageClassification", "image-classification"),
        ("ForCTC", "automatic-speech-recognition"),
        ("ForSemanticSegmentation", "image-segmentation"),
        ("ForObjectDetection", "object-detection"),
        ("ForAudioClassification", "audio-classification"),
    ];
    for (suf, t) in suffixes {
        if a.ends_with(suf) {
            return Some(t);
        }
    }
    if let Some(stem) = a.strip_suffix("ForConditionalGeneration") {
        if SEQ2SEQ_TEXT.contains(&stem) {
            return Some("text2text-generation");
        }
        if SPEECH.contains(&stem) {
            return Some("automatic-speech-recognition");
        }
        if VISION_CHAT.contains(&stem) {
            return Some("image-text-to-text");
        }
        return None;
    }
    if let Some(stem) = a.strip_suffix("Model")
        && ENCODERS.contains(&stem)
    {
        return Some("feature-extraction");
    }
    None
}

/// **The task a GGUF's `general.architecture` names** (llama.cpp's names): a decoder LLM → text generation; a vision LLM's
/// language part (`qwen2vl`, `qwen3vl`…) → its chat task; a sentence encoder → feature extraction; a diffusion model → text to
/// image. `None` for a name not listed.
pub fn task_of_gguf_architecture(g: &str) -> Option<&'static str> {
    const DECODERS: &[&str] = &[
        "llama",
        "llama4",
        "mistral3",
        "mistral4",
        "qwen",
        "qwen2",
        "qwen3",
        "qwen35",
        "qwen2moe",
        "qwen3moe",
        "qwen35moe",
        "qwen3next",
        "gemma",
        "gemma2",
        "gemma3",
        "gemma3n",
        "gemma4",
        "phi2",
        "phi3",
        "phimoe",
        "falcon",
        "falcon-h1",
        "granite",
        "granitemoe",
        "granitehybrid",
        "gpt2",
        "gptj",
        "gptneox",
        "gpt-oss",
        "starcoder",
        "starcoder2",
        "olmo",
        "olmo2",
        "olmoe",
        "exaone",
        "exaone4",
        "glm4",
        "glm4moe",
        "chatglm",
        "command-r",
        "cohere2",
        "mamba",
        "mamba2",
        "rwkv6",
        "rwkv7",
        "internlm2",
        "minicpm",
        "minicpm3",
        "stablelm",
        "lfm2",
        "lfm2moe",
        "deepseek",
        "deepseek2",
        "deepseek4",
        "nemotron",
        "nemotron_h",
        "nemotron_h_moe",
        "minimax-m2",
        "hunyuan-moe",
        "hunyuan-dense",
        "bailingmoe",
        "bailingmoe2",
        "ernie4_5",
        "ernie4_5-moe",
        "dots1",
        "arcee",
        "smollm3",
        "seed_oss",
        "apertus",
        "jamba",
        "plamo",
        "plamo2",
        "orion",
        "baichuan",
        "bloom",
        "mpt",
        "refact",
        "persimmon",
        "jais",
        "dbrx",
        "arctic",
        "openelm",
        "bitnet",
    ];
    const VISION_CHAT: &[&str] = &["qwen2vl", "qwen3vl", "qwen3vlmoe", "qwen25vl"];
    const ENCODERS: &[&str] =
        &["bert", "nomic-bert", "nomic-bert-moe", "jina-bert-v2", "jina-bert-v3", "modern-bert", "neo-bert", "t5encoder"];
    const IMAGE: &[&str] = &["flux", "sd1", "sd3", "sdxl", "lumina2", "qwen_image", "hidream", "wan", "chroma"];
    if DECODERS.contains(&g) {
        return Some("text-generation");
    }
    if VISION_CHAT.contains(&g) {
        return Some("image-text-to-text");
    }
    if ENCODERS.contains(&g) {
        return Some("feature-extraction");
    }
    if g == "t5" {
        return Some("text2text-generation");
    }
    if IMAGE.contains(&g) {
        return Some("text-to-image");
    }
    None
}

// ---- the selected artifact ------------------------------------------------------------------------------------------------------

/// The kind of the selected artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A Hugging Face checkpoint: `config.json` beside `model.safetensors` or `model.safetensors.index.json` and its shards.
    Safetensors,
    /// Safetensors files under names the transformers convention does not use (a single-file checkpoint, a LoRA file).
    SafetensorsOther,
    /// One GGUF file (or one split set).
    Gguf,
    /// A diffusers pipeline (`model_index.json` and its components).
    Diffusers,
    /// A diffusers component at the root (`config.json` with `_class_name` beside `diffusion_pytorch_model.safetensors`).
    DiffusersComponent,
    /// An adapter (PEFT `adapter_config.json`, a diffusers LoRA) over a base.
    Adapter,
    /// Weights only in a format the converter does not read.
    Other,
    /// No weight file.
    None,
}

/// The one artifact a repository is judged by.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedV1 {
    pub kind: ArtifactKind,
    /// The weight format(s) present (`safetensors`, `gguf`, `pytorch`, `onnx`, `tensorflow`, `flax`, …), sorted.
    pub formats: Vec<String>,
    /// The directory the artifact lives in (`""` for the root).
    pub dir: String,
    /// The configuration beside it, when there is one.
    pub config: Option<String>,
    /// The safetensors index, when there is one.
    pub index: Option<String>,
    /// The weight files (the shards the selection could name from the listing; the index's are authoritative once read).
    pub weights: Vec<String>,
    /// A diffusers pipeline's component configurations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    /// Why this artifact: the rule that chose it.
    pub rule: String,
}

/// The weight format of a file name, or `None` for a file that holds no weights.
pub fn weight_format(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    Some(match ext {
        "safetensors" => "safetensors",
        "gguf" => "gguf",
        "bin" if lower.starts_with("ggml") => "ggml",
        "bin" if lower.contains("openvino") => "openvino",
        "bin"
            if lower == "training_args.bin"
                || lower == "optimizer.bin"
                || lower == "scheduler.bin"
                || lower.starts_with("rng_state") =>
        {
            return None;
        }
        "bin" | "pt" | "pth" | "ckpt" => "pytorch",
        "onnx" | "ort" => "onnx",
        "h5" | "keras" | "tflite" => "tensorflow",
        "pb" if lower == "saved_model.pb" => "tensorflow",
        "msgpack" => "flax",
        "mlmodel" | "mlpackage" => "coreml",
        "nemo" => "nemo",
        "pkl" | "pickle" | "joblib" | "sav" => "pickle",
        "zip" => "zip",
        "npz" | "npy" => "numpy",
        "engine" | "plan" | "trt" => "tensorrt",
        "pte" => "executorch",
        "llamafile" => "llamafile",
        "ggml" => "ggml",
        _ => return None,
    })
}

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn file_of(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// A preference rank for a GGUF file by the type its name announces: the described integer types first (Q8_0 … Q4_0), then the
/// k-quants by size, the i-quants, the floats; an `mmproj` projector is never the artifact. Ties break by path.
pub fn gguf_rank(path: &str) -> u32 {
    let n = file_of(path).to_ascii_uppercase();
    const ORDER: &[&str] = &[
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_K", "Q4_K_M", "Q4_K_S", "Q4_K", "Q5_0", "Q5_1", "Q4_0", "Q4_1", "Q3_K_L", "Q3_K_M",
        "Q3_K_S", "Q3_K", "Q2_K", "IQ4_XS", "IQ4_NL", "IQ3_M", "IQ3_S", "IQ3_XS", "IQ3_XXS", "IQ2_M", "IQ2_S", "IQ2_XS", "IQ2_XXS",
        "IQ1_M", "IQ1_S", "TQ2_0", "TQ1_0", "F16", "BF16", "F32",
    ];
    ORDER.iter().position(|t| n.contains(t)).map(|p| p as u32).unwrap_or(ORDER.len() as u32)
}

/// `-00001-of-00003.gguf`: a split GGUF's part number and count.
pub fn gguf_split(path: &str) -> Option<(u32, u32)> {
    let n = file_of(path);
    let stem = n.strip_suffix(".gguf")?;
    let (head, count) = stem.rsplit_once("-of-")?;
    let part = head.rsplit_once('-')?.1;
    Some((part.parse().ok()?, count.parse().ok()?))
}

/// **The selection rule** (published with the census): a diffusers pipeline's `model_index.json`; else an adapter's
/// `adapter_config.json` when the root holds no full checkpoint of its own; else the transformers checkpoint at the root
/// (`config.json` beside `model.safetensors.index.json` or `model.safetensors`); else a diffusers component at the root; else the
/// shallowest directory holding such a checkpoint; else other safetensors files at the root; else the best-ranked single GGUF
/// ([`gguf_rank`]; a split set only when no single file exists); else the other formats present; else none.
pub fn select(l: &ListingV1) -> SelectedV1 {
    let sib: BTreeSet<&str> = l.siblings.iter().map(String::as_str).collect();
    let mut formats: BTreeSet<&'static str> = BTreeSet::new();
    for s in &l.siblings {
        if let Some(f) = weight_format(s) {
            formats.insert(f);
        }
    }
    let formats: Vec<String> = formats.into_iter().map(str::to_string).collect();
    let mk = |kind, dir: &str, config: Option<&str>, index: Option<&str>, weights: Vec<String>, rule: &str| SelectedV1 {
        kind,
        formats: formats.clone(),
        dir: dir.to_string(),
        config: config.map(str::to_string),
        index: index.map(str::to_string),
        weights,
        components: Vec::new(),
        rule: rule.to_string(),
    };
    let root_st: Vec<String> = l.siblings.iter().filter(|s| !s.contains('/') && s.ends_with(".safetensors")).cloned().collect();
    let has_root_full = sib.contains("model.safetensors")
        || sib.contains("model.safetensors.index.json")
        || sib.contains("pytorch_model.bin")
        || sib.contains("pytorch_model.bin.index.json");

    if sib.contains("model_index.json") {
        let comps: Vec<String> = l
            .siblings
            .iter()
            .filter(|s| s.matches('/').count() == 1 && (s.ends_with("/config.json") || s.ends_with("/scheduler_config.json")))
            .cloned()
            .collect();
        let mut s = mk(ArtifactKind::Diffusers, "", Some("model_index.json"), None, vec![], "a diffusers pipeline: model_index.json");
        s.components = comps;
        return s;
    }
    if sib.contains("adapter_config.json") && !has_root_full {
        let w: Vec<String> =
            ["adapter_model.safetensors", "adapter_model.bin"].iter().filter(|f| sib.contains(**f)).map(|f| f.to_string()).collect();
        return mk(
            ArtifactKind::Adapter,
            "",
            Some("adapter_config.json"),
            None,
            w,
            "a PEFT adapter: adapter_config.json, no full checkpoint",
        );
    }
    if sib.contains("config.json") && sib.contains("model.safetensors.index.json") {
        let w = root_st.iter().filter(|s| s.starts_with("model")).cloned().collect();
        return mk(
            ArtifactKind::Safetensors,
            "",
            Some("config.json"),
            Some("model.safetensors.index.json"),
            w,
            "transformers: config.json + model.safetensors.index.json",
        );
    }
    if sib.contains("config.json") && sib.contains("model.safetensors") {
        return mk(
            ArtifactKind::Safetensors,
            "",
            Some("config.json"),
            None,
            vec!["model.safetensors".into()],
            "transformers: config.json + model.safetensors",
        );
    }
    if sib.contains("config.json") && sib.contains("diffusion_pytorch_model.safetensors") {
        return mk(
            ArtifactKind::DiffusersComponent,
            "",
            Some("config.json"),
            None,
            vec!["diffusion_pytorch_model.safetensors".into()],
            "a diffusers component at the root",
        );
    }
    // A diffusers LoRA (a safetensors file the Hub relates to its base as an adapter, with no configuration of its own).
    if l.base_relation.as_deref() == Some("adapter") && !has_root_full && !root_st.is_empty() && !sib.contains("config.json") {
        return mk(
            ArtifactKind::Adapter,
            "",
            None,
            None,
            root_st.clone(),
            "an adapter the Hub relates to its base (no configuration)",
        );
    }
    // The shallowest directory holding a transformers checkpoint.
    let mut nested: Vec<&str> = l
        .siblings
        .iter()
        .filter(|s| s.contains('/') && (s.ends_with("/model.safetensors") || s.ends_with("/model.safetensors.index.json")))
        .map(|s| dir_of(s))
        .filter(|d| sib.contains(format!("{d}/config.json").as_str()))
        .collect();
    nested.sort_by_key(|d| (d.matches('/').count(), *d));
    nested.dedup();
    if let Some(d) = nested.first() {
        let index = format!("{d}/model.safetensors.index.json");
        let has_index = sib.contains(index.as_str());
        let w = l
            .siblings
            .iter()
            .filter(|s| dir_of(s) == *d && s.ends_with(".safetensors") && file_of(s).starts_with("model"))
            .cloned()
            .collect();
        return mk(
            ArtifactKind::Safetensors,
            d,
            Some(&format!("{d}/config.json")),
            has_index.then_some(index.as_str()),
            w,
            "transformers in a subdirectory: the shallowest, then the first by name",
        );
    }
    if !root_st.is_empty() {
        let config = sib.contains("config.json").then_some("config.json");
        return mk(ArtifactKind::SafetensorsOther, "", config, None, root_st, "safetensors files at the root under other names");
    }
    let ggufs: Vec<&String> =
        l.siblings.iter().filter(|s| s.ends_with(".gguf") && !file_of(s).to_ascii_lowercase().contains("mmproj")).collect();
    if !ggufs.is_empty() {
        let single: Vec<&&String> = ggufs.iter().filter(|s| gguf_split(s).is_none()).collect();
        if let Some(best) = single.iter().min_by_key(|s| (gguf_rank(s), s.as_str())) {
            return mk(ArtifactKind::Gguf, dir_of(best), None, None, vec![best.to_string()], "the best-ranked single GGUF file");
        }
        // Only split sets: the best-ranked set, every part.
        let first = ggufs.iter().filter(|s| gguf_split(s).is_some_and(|(p, _)| p == 1)).min_by_key(|s| (gguf_rank(s), s.as_str()));
        if let Some(first) = first {
            let prefix = first.rfind("-00001-of-").map(|i| &first[..i]).unwrap_or(first.as_str());
            let parts: Vec<String> =
                ggufs.iter().filter(|s| s.starts_with(prefix) && gguf_split(s).is_some()).map(|s| s.to_string()).collect();
            return mk(ArtifactKind::Gguf, dir_of(first), None, None, parts, "a split GGUF set (no single file)");
        }
        return mk(ArtifactKind::Gguf, "", None, None, vec![ggufs[0].clone()], "a GGUF file");
    }
    if !formats.is_empty() {
        return mk(
            ArtifactKind::Other,
            "",
            sib.contains("config.json").then_some("config.json"),
            None,
            vec![],
            "weights only in other formats",
        );
    }
    mk(ArtifactKind::None, "", None, None, vec![], "no weight file")
}

// ---- strata ----------------------------------------------------------------------------------------------------------------------

/// The size band of a parameter count.
pub fn size_band(params: Option<u64>) -> &'static str {
    match params {
        None => "unknown",
        Some(p) if p < 100_000_000 => "<100M",
        Some(p) if p < 1_000_000_000 => "100M-1B",
        Some(p) if p < 10_000_000_000 => "1B-10B",
        Some(p) if p < 100_000_000_000 => "10B-100B",
        Some(_) => ">=100B",
    }
}

/// The feature family a report groups by: the transformers `model_type`, the GGUF architecture, the diffusers class, or the library.
pub fn family_of(l: &ListingV1) -> String {
    if let Some(m) = l.model_type() {
        return m.to_ascii_lowercase();
    }
    if let Some(a) = &l.gguf_arch {
        return format!("gguf:{a}");
    }
    if let Some(c) = l.config.get("diffusers").and_then(|d| d.get("_class_name")).and_then(|c| c.as_str()) {
        return format!("diffusers:{c}");
    }
    if l.config.get("peft").is_some() {
        return "peft".into();
    }
    match &l.library {
        Some(lib) => format!("lib:{lib}"),
        None => "unknown".into(),
    }
}

/// The strata of one repository (§II.10.1: task, library, size, format, age, feature family; `unknown` is a stratum).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrataV1 {
    pub task: String,
    pub task_group: String,
    pub library: String,
    pub format: String,
    pub size_band: String,
    pub family: String,
    pub year: String,
}

pub fn strata_of(l: &ListingV1, task: &TaskV1, sel: &SelectedV1) -> StrataV1 {
    let format = match sel.kind {
        ArtifactKind::Safetensors | ArtifactKind::SafetensorsOther => "safetensors".to_string(),
        ArtifactKind::Gguf => "gguf".into(),
        ArtifactKind::Diffusers | ArtifactKind::DiffusersComponent => "diffusers".into(),
        ArtifactKind::Adapter => "adapter".into(),
        ArtifactKind::Other => format!("other:{}", sel.formats.first().cloned().unwrap_or_default()),
        ArtifactKind::None => "none".into(),
    };
    StrataV1 {
        task: task.task.clone(),
        task_group: task.group.clone(),
        library: l.library.clone().unwrap_or_else(|| "unknown".into()),
        format,
        size_band: size_band(l.params()).to_string(),
        family: family_of(l),
        year: l.created.as_deref().and_then(|c| c.get(..4)).unwrap_or("unknown").to_string(),
    }
}

// ---- the fetch plan ----------------------------------------------------------------------------------------------------------------

/// What a header fetch of the repository must read (the fetcher executes it; nothing else is read).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanV1 {
    /// Small metadata files read whole (configuration, indices, the pipeline index, the adapter configuration).
    pub files: Vec<String>,
    /// Safetensors files whose header (8-byte length and JSON) is read. When `from_index` is set, the shards the index names.
    pub st_headers: Vec<String>,
    pub st_from_index: Option<String>,
    /// GGUF files whose metadata and tensor infos are read.
    pub gguf_headers: Vec<String>,
    /// An adapter's base, pinned in the snapshot: its configuration, index and shard headers are read too (stored under `base/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<BaseRefV1>,
}

/// A repository at a commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseRefV1 {
    pub repo: String,
    pub sha: String,
}

pub fn plan_of(l: &ListingV1, sel: &SelectedV1) -> PlanV1 {
    let mut p = PlanV1::default();
    if sel.kind == ArtifactKind::Adapter
        && sel.config.is_some()
        && let Ok(b) = pinned_base(l)
        && let Some(sha) = &b.sha
    {
        p.base = Some(BaseRefV1 { repo: b.id.clone(), sha: sha.clone() });
    }
    match sel.kind {
        ArtifactKind::Safetensors => {
            p.files.extend(sel.config.iter().cloned());
            match &sel.index {
                Some(ix) => {
                    p.files.push(ix.clone());
                    p.st_from_index = Some(ix.clone());
                }
                None => p.st_headers.extend(sel.weights.iter().cloned()),
            }
        }
        ArtifactKind::SafetensorsOther | ArtifactKind::DiffusersComponent => {
            p.files.extend(sel.config.iter().cloned());
            p.st_headers.extend(sel.weights.iter().take(16).cloned());
        }
        ArtifactKind::Diffusers => {
            p.files.push("model_index.json".into());
            p.files.extend(sel.components.iter().cloned());
        }
        ArtifactKind::Adapter => {
            p.files.extend(sel.config.iter().cloned());
            p.st_headers.extend(sel.weights.iter().filter(|w| w.ends_with(".safetensors")).take(4).cloned());
        }
        // A split GGUF set is not read by this build (FORMAT_UNSUPPORTED on the listing): its parts' headers are not fetched.
        ArtifactKind::Gguf if sel.weights.len() == 1 => p.gguf_headers.extend(sel.weights.iter().cloned()),
        ArtifactKind::Gguf => {}
        ArtifactKind::Other | ArtifactKind::None => {}
    }
    p
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_adapter_s_base_is_its_peft_config_s_through_a_rename_and_never_a_guess() {
        let mut l = listing(&["adapter_config.json", "adapter_model.safetensors"]);
        let b = |id: &str, from: Option<&str>| super::BaseResolvedV1 {
            id: id.into(),
            found: true,
            sha: Some("s".into()),
            gated: false,
            disabled: false,
            license: None,
            renamed_from: from.map(str::to_string),
        };
        assert_eq!(super::pinned_base(&l).unwrap_err().0, "absent");
        l.base_ids = vec!["a/x".into(), "b/y".into()];
        l.base_resolved = vec![b("a/x", None), b("b/y", None)];
        assert_eq!(super::pinned_base(&l).unwrap_err().0, "ambiguous");
        l.config = serde_json::json!({"peft": {"base_model_name_or_path": "b/y"}});
        assert_eq!(super::pinned_base(&l).unwrap().id, "b/y");
        l.config = serde_json::json!({"peft": {"base_model_name_or_path": "old/name"}});
        l.base_resolved.push(b("new/name", Some("old/name")));
        assert_eq!(super::pinned_base(&l).unwrap().id, "new/name");
    }

    #[test]
    fn a_missing_task_is_inferred_from_a_head_class_or_a_gguf_architecture_and_nothing_else() {
        use super::{task_of_architecture_class as a, task_of_gguf_architecture as g};
        assert_eq!(a("LlamaForCausalLM"), Some("text-generation"));
        assert_eq!(a("BertForSequenceClassification"), Some("text-classification"));
        assert_eq!(a("T5ForConditionalGeneration"), Some("text2text-generation"));
        assert_eq!(a("WhisperForConditionalGeneration"), Some("automatic-speech-recognition"));
        assert_eq!(a("Qwen2_5_VLForConditionalGeneration"), Some("image-text-to-text"));
        assert_eq!(a("XLMRobertaModel"), Some("feature-extraction"));
        assert_eq!(a("CustomResearchModel"), None, "a custom class names no task");
        assert_eq!(a("SomethingForConditionalGeneration"), None, "an unlisted conditional-generation class is not guessed");
        assert_eq!(g("llama"), Some("text-generation"));
        assert_eq!(g("qwen3vl"), Some("image-text-to-text"));
        assert_eq!(g("nomic-bert"), Some("feature-extraction"));
        assert_eq!(g("flux"), Some("text-to-image"));
        assert_eq!(g("clip"), None);
    }

    #[test]
    fn a_peft_adapter_declares_its_task_by_the_head_it_was_trained_with() {
        use super::task_of_peft_task_type as p;
        assert_eq!(p("CAUSAL_LM"), Some("text-generation"));
        assert_eq!(p("SEQ_2_SEQ_LM"), Some("text2text-generation"));
        assert_eq!(p("SEQ_CLS"), Some("text-classification"));
        assert_eq!(p("TOKEN_CLS"), Some("token-classification"));
        assert_eq!(p("QUESTION_ANS"), Some("question-answering"));
        assert_eq!(p("FEATURE_EXTRACTION"), Some("feature-extraction"));
        assert_eq!(p("SOMETHING_NEW"), None);
        let mut l = listing(&["adapter_config.json", "adapter_model.safetensors"]);
        l.config = serde_json::json!({"peft": {"task_type": "SEQ_CLS"}});
        let t = task_of(&l);
        assert_eq!((t.task.as_str(), t.source.as_str(), t.profile), ("text-classification", "inferred:peft", Profile::None));
        // No declared task: unknown until the pinned base is read.
        l.config = serde_json::json!({"peft": {"base_model_name_or_path": "b/base"}});
        assert_eq!(task_of(&l).task, "unknown");
    }

    use super::*;

    fn listing(siblings: &[&str]) -> ListingV1 {
        ListingV1 { id: "o/n".into(), siblings: siblings.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn the_transformers_checkpoint_at_the_root_is_selected_before_a_gguf_beside_it() {
        let l = listing(&[
            "config.json",
            "model.safetensors.index.json",
            "model-00001-of-00002.safetensors",
            "model-00002-of-00002.safetensors",
            "x.Q4_K_M.gguf",
        ]);
        let s = select(&l);
        assert_eq!(s.kind, ArtifactKind::Safetensors);
        assert_eq!(s.index.as_deref(), Some("model.safetensors.index.json"));
        assert_eq!(s.formats, vec!["gguf".to_string(), "safetensors".to_string()]);
        let p = plan_of(&l, &s);
        assert_eq!(p.st_from_index.as_deref(), Some("model.safetensors.index.json"));
        assert!(p.files.contains(&"config.json".to_string()));
    }

    #[test]
    fn a_gguf_repository_selects_one_described_single_file_and_never_a_projector() {
        let l = listing(&["m-F16.gguf", "m-Q4_K_M.gguf", "m-Q8_0.gguf", "mmproj-m-Q8_0.gguf", "README.md"]);
        let s = select(&l);
        assert_eq!(s.kind, ArtifactKind::Gguf);
        assert_eq!(s.weights, vec!["m-Q8_0.gguf".to_string()]);
        let l = listing(&["big-Q4_K_M-00001-of-00002.gguf", "big-Q4_K_M-00002-of-00002.gguf"]);
        let s = select(&l);
        assert_eq!(s.weights.len(), 2, "a split set is read whole: {s:?}");
        assert_eq!(gguf_split("a/b-Q8_0-00002-of-00003.gguf"), Some((2, 3)));
    }

    #[test]
    fn an_adapter_without_a_checkpoint_of_its_own_is_an_adapter() {
        let l = listing(&["adapter_config.json", "adapter_model.safetensors", "README.md"]);
        assert_eq!(select(&l).kind, ArtifactKind::Adapter);
        let mut l = listing(&["pytorch_lora_weights.safetensors"]);
        l.base_relation = Some("adapter".into());
        assert_eq!(select(&l).kind, ArtifactKind::Adapter);
        let l = listing(&["adapter_config.json", "config.json", "model.safetensors"]);
        assert_eq!(select(&l).kind, ArtifactKind::Safetensors, "a merged checkpoint beside its adapter is the checkpoint");
    }

    #[test]
    fn other_formats_and_no_weights_are_named() {
        let s = select(&listing(&["config.json", "pytorch_model.bin", "training_args.bin"]));
        assert_eq!((s.kind, s.formats.clone()), (ArtifactKind::Other, vec!["pytorch".to_string()]));
        let s = select(&listing(&["README.md", ".gitattributes", "training_args.bin"]));
        assert_eq!(s.kind, ArtifactKind::None);
        let s = select(&listing(&["onnx/model.onnx", "config.json"]));
        assert_eq!(s.formats, vec!["onnx".to_string()]);
    }

    #[test]
    fn a_nested_checkpoint_is_the_shallowest_then_the_first() {
        let l = listing(&["b/config.json", "b/model.safetensors", "a/x/config.json", "a/x/model.safetensors", "c/model.safetensors"]);
        let s = select(&l);
        assert_eq!((s.kind, s.dir.as_str()), (ArtifactKind::Safetensors, "b"));
    }

    #[test]
    fn the_task_is_declared_or_inferred_from_the_head_class() {
        let mut l = listing(&[]);
        assert_eq!(task_of(&l).task, "unknown");
        l.config = serde_json::json!({"architectures": ["FooForCausalLM"]});
        assert_eq!((task_of(&l).task.as_str(), task_of(&l).source.as_str()), ("text-generation", "inferred:architecture"));
        // Inference v2: a head class names its task (still no profile for classification).
        l.config = serde_json::json!({"architectures": ["FooForSequenceClassification"]});
        assert_eq!((task_of(&l).task.as_str(), task_of(&l).source.as_str()), ("text-classification", "inferred:architecture-class"));
        assert_eq!(task_of(&l).profile, Profile::None);
        l.config = serde_json::json!({"architectures": ["FooResearchModel"]});
        assert_eq!(task_of(&l).task, "unknown");
        l.pipeline_tag = Some("image-classification".into());
        assert_eq!(task_of(&l).profile, Profile::None);
    }
}
