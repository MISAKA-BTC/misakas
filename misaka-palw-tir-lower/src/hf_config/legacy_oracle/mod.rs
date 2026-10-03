//! **The per-architecture Rust parsers** — the differential oracle (feature `legacy-oracle`).
//!
//! One parser per Hugging Face architecture, as this crate had them before model adapters became
//! data. They are compiled only with `--features legacy-oracle` and used by one test file,
//! `tests/adapters.rs`, which holds every built-in adapter (`adapters/*.json`) to the `ModelSpec`
//! these functions produce on every fixture, every published config and ~15 000 single-key mutants
//! of them. The permanent regression gate is `tests/golden_lowering.rs` (programs and artifacts);
//! when the corpus lane has also validated the adapters this directory can be deleted.

mod dense;
mod encoder;
mod hybrid;
mod legacy;
mod moe;

use super::{attach_quant, remote_module};
use crate::cfg::Cfg;
use crate::error::{LowerError, Result};
use crate::rope::{RopeSpec, RopeStyle};
use crate::spec::*;
use serde_json::Value;
use std::collections::BTreeMap;

/// Every `architectures[0]` this lowerer lowers, with the corpus families it exercises. (RWKV-5/6/7
/// are dispatched to parsers that refuse them with the reason.)
pub const SUPPORTED: &[(&str, &str)] = &[
    ("LlamaForCausalLM", "C1 C2"),
    ("MistralForCausalLM", "C1 C2"),
    ("MinistralForCausalLM", "C1 C2"),
    ("Ministral3ForCausalLM", "C1 C2"),
    ("GlmForCausalLM", "C1"),
    ("Glm4ForCausalLM", "C1"),
    ("Olmo3ForCausalLM", "C1 C2"),
    ("Qwen2ForCausalLM", "C1 C2"),
    ("Qwen3ForCausalLM", "C1 C2"),
    ("GemmaForCausalLM", "C1"),
    ("Gemma2ForCausalLM", "C1 C2"),
    ("Gemma3ForCausalLM", "C1 C2"),
    ("Gemma3ForConditionalGeneration", "C1 C2 (text decoder only)"),
    ("LlavaForConditionalGeneration", "C1 (text decoder only)"),
    ("Mistral3ForConditionalGeneration", "C1 (text decoder only)"),
    ("PhiForCausalLM", "C1"),
    ("Phi3ForCausalLM", "C1 C2"),
    ("GPT2LMHeadModel", "C1"),
    ("GPTNeoForCausalLM", "C1 C2"),
    ("GPTNeoXForCausalLM", "C1"),
    ("GPTJForCausalLM", "C1"),
    ("FalconForCausalLM", "C1 C2"),
    ("RWForCausalLM", "C1 C2"),
    ("StableLmForCausalLM", "C1"),
    ("Starcoder2ForCausalLM", "C1 C2"),
    ("GPTBigCodeForCausalLM", "C1 C2"),
    ("OlmoForCausalLM", "C1"),
    ("Olmo2ForCausalLM", "C1"),
    ("CohereForCausalLM", "C1"),
    ("Cohere2ForCausalLM", "C1 C2"),
    ("InternLM2ForCausalLM", "C1 C2"),
    ("GraniteForCausalLM", "C1"),
    ("MiniCPMForCausalLM", "C1"),
    ("NemotronForCausalLM", "C1"),
    ("ExaoneForCausalLM", "C1 C2"),
    ("Exaone4ForCausalLM", "C1 C2"),
    ("SmolLM3ForCausalLM", "C1 C2"),
    ("BloomForCausalLM", "C1"),
    ("MPTForCausalLM", "C1"),
    ("MptForCausalLM", "C1"),
    ("OPTForCausalLM", "C1"),
    ("MixtralForCausalLM", "C3 C8"),
    ("Qwen2MoeForCausalLM", "C3 C8"),
    ("Qwen3MoeForCausalLM", "C3 C8"),
    ("OlmoeForCausalLM", "C3 C8"),
    ("GraniteMoeForCausalLM", "C3 C8"),
    ("DeepseekV2ForCausalLM", "C3 C8 (MLA)"),
    ("DeepseekV3ForCausalLM", "C3 C8 (MLA)"),
    ("Glm4MoeForCausalLM", "C3 C8"),
    ("PhimoeForCausalLM", "C3 C8"),
    ("Llama4ForCausalLM", "C1 C3 C8"),
    ("Gemma4ForCausalLM", "C1 C2 (C3 C8 with its MoE block)"),
    ("Llama4ForConditionalGeneration", "C1 C3 C8 (text decoder only)"),
    ("GptOssForCausalLM", "C2 C3 C8"),
    ("Qwen3NextForCausalLM", "C3 C4 C7"),
    ("Qwen3_5ForCausalLM", "C4 C7"),
    ("Qwen3_5MoeForCausalLM", "C3 C4 C7"),
    ("Qwen3_5ForConditionalGeneration", "C4 C7 (text decoder only)"),
    ("Qwen3_5MoeForConditionalGeneration", "C3 C4 C7 (text decoder only)"),
    ("JambaForCausalLM", "C3 C5 C7"),
    ("MambaForCausalLM", "C5"),
    ("FalconMambaForCausalLM", "C5"),
    ("Mamba2ForCausalLM", "C5"),
    ("RwkvForCausalLM", "C6"),
    ("CLIPTextModel", "E1 (encoder: RFC-0003 Embedding profile)"),
    ("CLIPTextModelWithProjection", "E1 (encoder: RFC-0003 Embedding profile)"),
    ("BertModel", "E2 (bidirectional encoder: RFC-0003 Embedding profile)"),
    ("RobertaModel", "E2 (bidirectional encoder: RFC-0003 Embedding profile)"),
    ("MPNetModel", "E2 (bidirectional encoder: RFC-0003 Embedding profile)"),
    ("MPNetForMaskedLM", "E2 (bidirectional encoder, the MLM head ignored)"),
    ("DistilBertModel", "E2 (bidirectional encoder: RFC-0003 Embedding profile)"),
    ("DistilBertForMaskedLM", "E2 (bidirectional encoder, the MLM head ignored)"),
    ("XLMRobertaModel", "E2 (bidirectional encoder: RFC-0003 Embedding profile)"),
];

/// Architectures refused on purpose, with the reason (printed instead of "unknown").
pub const REFUSED: &[(&str, &str)] = &[
    ("Gemma3nForConditionalGeneration", "AltUp/Laurel/per-layer embeddings and activation sparsity are not modelled yet"),
    ("Gemma3nForCausalLM", "AltUp/Laurel/per-layer embeddings and activation sparsity are not modelled yet"),
    ("MllamaForConditionalGeneration", "the text decoder has cross-attention layers that read vision states"),
    ("PaliGemmaForConditionalGeneration", "prefix-LM (bidirectional) attention over the prompt"),
    ("Phi3SmallForCausalLM", "blocksparse attention and muP scalings (remote code) are not modelled"),
    ("ChatGLMModel", "ChatGLM remote code is not modelled"),
    ("ChatGLMForConditionalGeneration", "ChatGLM remote code is not modelled"),
    ("CodeGenForCausalLM", "CodeGen's mp_num-partitioned qkv layout is not modelled"),
    ("MiniCPM3ForCausalLM", "MiniCPM3 (MLA + muP remote code) is not modelled yet"),
    ("GraniteMoeHybridForCausalLM", "Granite-4 hybrid (Mamba2 + attention + MoE) is not modelled yet"),
    ("NemotronHForCausalLM", "Nemotron-H hybrid is not modelled yet"),
    ("FalconH1ForCausalLM", "Falcon-H1 hybrid with muP multipliers is not modelled yet"),
    ("Zamba2ForCausalLM", "Zamba2 shared-attention hybrid is not modelled yet"),
];

/// Remote-code modules whose math this lowerer models (the `AutoModelForCausalLM` target).
const MODELLED_REMOTE: &[(&str, &[&str])] = &[
    ("InternLM2ForCausalLM", &["modeling_internlm2.InternLM2ForCausalLM"]),
    ("MiniCPMForCausalLM", &["modeling_minicpm.MiniCPMForCausalLM"]),
    ("ExaoneForCausalLM", &["modeling_exaone.ExaoneForCausalLM"]),
    ("Phi3ForCausalLM", &["modeling_phi3.Phi3ForCausalLM"]),
    ("MPTForCausalLM", &["modeling_mpt.MPTForCausalLM"]),
    ("DeepseekV2ForCausalLM", &["modeling_deepseek.DeepseekV2ForCausalLM"]),
    ("DeepseekV3ForCausalLM", &["modeling_deepseek.DeepseekV3ForCausalLM"]),
    ("RWForCausalLM", &["modelling_RW.RWForCausalLM", "modeling_RW.RWForCausalLM"]),
    ("FalconForCausalLM", &["modeling_falcon.FalconForCausalLM"]),
    ("Rwkv5ForCausalLM", &["modeling_rwkv5.Rwkv5ForCausalLM"]),
    ("Rwkv6ForCausalLM", &["modeling_rwkv6.Rwkv6ForCausalLM"]),
];

/// Architectures that exist ONLY as remote code: without `auto_map` there is nothing to model.
const REMOTE_ONLY: &[&str] =
    &["InternLM2ForCausalLM", "MiniCPMForCausalLM", "ExaoneForCausalLM", "RWForCausalLM", "Rwkv5ForCausalLM", "Rwkv6ForCausalLM"];

/// The per-architecture Rust parsers: the Level B path while its families are being converted into
/// adapter data files ([`crate::hf_schema`]).
#[doc(hidden)]
pub fn parse_legacy(v: &Value) -> Result<ArchSpec> {
    let root = v.as_object().ok_or_else(|| LowerError::bad("config.json is not an object"))?;
    let arch = match root.get("architectures") {
        Some(Value::Array(a)) if !a.is_empty() => {
            a[0].as_str().ok_or_else(|| LowerError::bad("architectures[0] is not a string"))?.to_string()
        }
        _ => {
            return Err(LowerError::not_lowerable(
                "config has no `architectures`: the reference implementation cannot be identified (not a transformers config?)",
            ));
        }
    };
    if let Some((_, why)) = REFUSED.iter().find(|(a, _)| *a == arch) {
        return Err(LowerError::not_lowerable(format!("{arch}: {why}")));
    }
    if root.get("is_encoder_decoder").and_then(Value::as_bool) == Some(true) {
        return Err(LowerError::not_lowerable(format!("{arch}: encoder–decoder models are out of scope for v1")));
    }
    if root.get("add_cross_attention").and_then(Value::as_bool) == Some(true) {
        return Err(LowerError::not_lowerable(format!("{arch}: cross-attention is out of scope for v1")));
    }
    // A pre-quantised checkpoint (GPTQ, AWQ): its integers are lowered as stored
    // (`crate::prequant`); every other method is refused there.
    let quant = match root.get("quantization_config").filter(|q| !q.is_null()) {
        Some(q) => {
            let mt = root.get("model_type").and_then(Value::as_str).unwrap_or("");
            Some(crate::prequant::parse_quant_config(q, &arch, mt)?)
        }
        None => None,
    };
    let reference = match root.get("auto_map").and_then(Value::as_object) {
        Some(m) => {
            let module = remote_module(m).unwrap_or_default();
            let ok = MODELLED_REMOTE.iter().any(|(a, mods)| *a == arch && mods.contains(&module.as_str()));
            if !ok {
                return Err(LowerError::not_lowerable(format!(
                    "{arch}: trust_remote_code module `{module}` is not modelled — its forward may differ from any transformers class"
                )));
            }
            Reference::RemoteCode { module }
        }
        None => {
            if REMOTE_ONLY.contains(&arch.as_str()) {
                return Err(LowerError::not_lowerable(format!("{arch}: exists only as remote code and the config has no auto_map")));
            }
            Reference::Native
        }
    };

    let mut p = P {
        cfg: Cfg::new(arch.clone(), root, ""),
        notes: vec![],
        unsure: vec![],
        reference,
        layouts: Layouts::default(),
        ignored_prefixes: vec![],
    };
    p.cfg.inert(&["auto_map", "quantization_config", "is_encoder_decoder", "add_cross_attention"]);
    // Another library's settings (transformers.js: the ONNX export's dtype and kv-cache options,
    // SmolLM2's configs carry them); nothing in transformers reads the key.
    p.cfg.inert(&["transformers.js_config"]);
    let spec = match arch.as_str() {
        "LlamaForCausalLM" => dense::llama(&mut p, Flavor::Llama)?,
        "MistralForCausalLM" => dense::llama(&mut p, Flavor::Mistral)?,
        // Ministral (8B-2410): Mistral math with `layer_types` (all sliding by default).
        "MinistralForCausalLM" | "Ministral3ForCausalLM" => dense::llama(&mut p, Flavor::Mistral)?,
        "GlmForCausalLM" => dense::glm(&mut p, false)?,
        "Glm4ForCausalLM" => dense::glm(&mut p, true)?,
        "Olmo3ForCausalLM" => dense::olmo3(&mut p)?,
        "Qwen2ForCausalLM" => dense::llama(&mut p, Flavor::Qwen2)?,
        "Qwen3ForCausalLM" => dense::llama(&mut p, Flavor::Qwen3)?,
        "GraniteForCausalLM" => dense::llama(&mut p, Flavor::Granite)?,
        "MiniCPMForCausalLM" => dense::llama(&mut p, Flavor::MiniCpm)?,
        "SmolLM3ForCausalLM" => dense::llama(&mut p, Flavor::SmolLm3)?,
        "InternLM2ForCausalLM" => dense::internlm2(&mut p)?,
        "ExaoneForCausalLM" => dense::exaone(&mut p)?,
        "Exaone4ForCausalLM" => dense::exaone4(&mut p)?,
        "NemotronForCausalLM" => dense::nemotron(&mut p)?,
        "StableLmForCausalLM" => dense::stablelm(&mut p)?,
        "Starcoder2ForCausalLM" => dense::starcoder2(&mut p)?,
        "OlmoForCausalLM" => dense::olmo(&mut p)?,
        "Olmo2ForCausalLM" => dense::olmo2(&mut p)?,
        "CohereForCausalLM" => dense::cohere(&mut p, false)?,
        "Cohere2ForCausalLM" => dense::cohere(&mut p, true)?,
        "GemmaForCausalLM" => dense::gemma(&mut p)?,
        "Gemma2ForCausalLM" => dense::gemma2(&mut p)?,
        "Gemma3ForCausalLM" => dense::gemma3_text(&mut p, "model.", "lm_head", vec![])?,
        "Gemma3ForConditionalGeneration" | "Llama4ForConditionalGeneration" => vlm_text(&mut p, &arch)?,
        "LlavaForConditionalGeneration" | "Mistral3ForConditionalGeneration" => vlm_text(&mut p, &arch)?,
        "Phi3ForCausalLM" => dense::phi3(&mut p)?,
        "PhiForCausalLM" => legacy::phi(&mut p)?,
        "GPT2LMHeadModel" => legacy::gpt2(&mut p)?,
        "GPTNeoForCausalLM" => legacy::gpt_neo(&mut p)?,
        "GPTNeoXForCausalLM" => legacy::gpt_neox(&mut p)?,
        "GPTJForCausalLM" => legacy::gptj(&mut p)?,
        "FalconForCausalLM" | "RWForCausalLM" => legacy::falcon(&mut p)?,
        "GPTBigCodeForCausalLM" => legacy::gpt_bigcode(&mut p)?,
        "BloomForCausalLM" => legacy::bloom(&mut p)?,
        "MPTForCausalLM" | "MptForCausalLM" => legacy::mpt(&mut p)?,
        "OPTForCausalLM" => legacy::opt(&mut p)?,
        "MixtralForCausalLM" => moe::mixtral(&mut p)?,
        "Qwen2MoeForCausalLM" => moe::qwen_moe(&mut p, false)?,
        "Qwen3MoeForCausalLM" => moe::qwen_moe(&mut p, true)?,
        "OlmoeForCausalLM" => moe::olmoe(&mut p)?,
        "GraniteMoeForCausalLM" => moe::granite_moe(&mut p)?,
        "DeepseekV2ForCausalLM" => moe::deepseek(&mut p, 2)?,
        "DeepseekV3ForCausalLM" => moe::deepseek(&mut p, 3)?,
        "Glm4MoeForCausalLM" => moe::glm4_moe(&mut p)?,
        "PhimoeForCausalLM" => moe::phimoe(&mut p)?,
        "Llama4ForCausalLM" => moe::llama4_text(&mut p, "model.", "lm_head")?,
        "Gemma4ForCausalLM" => dense::gemma4_text(&mut p, "model.", "lm_head")?,
        "GptOssForCausalLM" => moe::gpt_oss(&mut p)?,
        "Qwen3NextForCausalLM" => hybrid::qwen3_next(&mut p)?,
        "Qwen3_5ForCausalLM" => hybrid::qwen3_5_text(&mut p, false, "model.", "lm_head", vec![])?,
        "Qwen3_5MoeForCausalLM" => hybrid::qwen3_5_text(&mut p, true, "model.", "lm_head", vec![])?,
        "Qwen3_5ForConditionalGeneration" | "Qwen3_5MoeForConditionalGeneration" => vlm_text(&mut p, &arch)?,
        "Qwen2VLForConditionalGeneration" | "Qwen2_5_VLForConditionalGeneration" => vlm_text(&mut p, &arch)?,
        "JambaForCausalLM" => hybrid::jamba(&mut p)?,
        "MambaForCausalLM" => hybrid::mamba(&mut p, false)?,
        "FalconMambaForCausalLM" => hybrid::mamba(&mut p, true)?,
        "Mamba2ForCausalLM" => hybrid::mamba2(&mut p)?,
        "RwkvForCausalLM" => hybrid::rwkv4(&mut p)?,
        "Rwkv5ForCausalLM" => hybrid::rwkv56(&mut p, 5)?,
        "Rwkv6ForCausalLM" => hybrid::rwkv56(&mut p, 6)?,
        "RWKV7ForCausalLM" => hybrid::rwkv7(&mut p)?,
        "CLIPTextModel" => encoder::clip_text(&mut p, false)?,
        "CLIPTextModelWithProjection" => encoder::clip_text(&mut p, true)?,
        "BertModel" => encoder::bert_like(&mut p, encoder::BertFlavor::Bert)?,
        "RobertaModel" | "XLMRobertaModel" => encoder::bert_like(&mut p, encoder::BertFlavor::Roberta)?,
        // sentence-transformers repositories keep the base checkpoint's `…ForMaskedLM` name
        // (all-mpnet-base-v2, distilbert-base): the encoder is read, the MLM head ignored.
        "MPNetModel" | "MPNetForMaskedLM" => encoder::bert_like(&mut p, encoder::BertFlavor::MPNet)?,
        "DistilBertModel" | "DistilBertForMaskedLM" => encoder::bert_like(&mut p, encoder::BertFlavor::DistilBert)?,
        other => {
            return Err(LowerError::not_lowerable(format!(
                "`{other}` has no lowerer template (encoder-only, encoder–decoder, vision/audio, or a decoder not modelled yet)"
            )));
        }
    };
    p.cfg.finish()?;
    let mut spec = spec;
    if let Some(q) = quant {
        attach_quant(&mut spec, q)?;
    }
    Ok(spec)
}

/// The text decoder of a VLM, lowered alone (text-only prompts). The vision tower, projector and
/// image tokens are out of scope; the decoder's math is unchanged when no image is present.
fn vlm_text(p: &mut P, arch: &str) -> Result<ArchSpec> {
    let text = p.cfg.opt_obj("text_config")?.ok_or_else(|| LowerError::bad(format!("{arch}: no text_config")))?;
    p.cfg.inert(&[
        "vision_config",
        "mm_tokens_per_image",
        "boi_token_index",
        "eoi_token_index",
        "image_token_index",
        "image_token_id",
        "vision_feature_layer",
        "vision_feature_select_strategy",
        "projector_hidden_act",
        "multimodal_projector_bias",
        "image_seq_length",
        "spatial_merge_size",
        "vision_tower_config",
        "ignore_index",
        "video_token_id",
        "video_token_index",
        "vision_start_token_id",
        "vision_end_token_id",
    ]);
    let tmt = text.get("model_type").and_then(Value::as_str).unwrap_or(match arch {
        "Gemma3ForConditionalGeneration" => "gemma3_text",
        "Mistral3ForConditionalGeneration" => "mistral",
        "Qwen3_5ForConditionalGeneration" => "qwen3_5_text",
        "Qwen3_5MoeForConditionalGeneration" => "qwen3_5_moe_text",
        "Llama4ForConditionalGeneration" => "llama4_text",
        _ => "llama",
    });
    // Both HF weight layouts exist on the hub: `language_model.model.*` (≤ 4.51) and
    // `model.language_model.*` (≥ 4.52, and every checkpoint of a newer family).
    let qwen35 = tmt.starts_with("qwen3_5");
    // Qwen2-VL and Qwen2.5-VL: `model.language_model.*` (≥ 4.52) or the plain Qwen2 names `model.*`
    // their original checkpoints (and transformers 5's save) use.
    let qwen2vl = matches!(tmt, "qwen2_vl_text" | "qwen2_5_vl_text");
    let (prefix, lm_head) = if qwen35 {
        ("model.language_model.", "lm_head")
    } else if qwen2vl {
        ("model.", "lm_head")
    } else {
        ("language_model.model.", "language_model.lm_head")
    };
    let aliases = if qwen2vl {
        vec![("model.".to_string(), "model.language_model.".to_string())]
    } else if qwen35 {
        vec![
            ("model.language_model.".to_string(), "language_model.model.".to_string()),
            ("lm_head".to_string(), "language_model.lm_head".to_string()),
        ]
    } else {
        vec![
            ("language_model.model.".to_string(), "model.language_model.".to_string()),
            ("language_model.lm_head".to_string(), "lm_head".to_string()),
        ]
    };
    let root_tie = p.cfg.opt_bool("tie_word_embeddings")?;
    // A text-only lowering never reads the vision tower, the projector or the MTP heads.
    let vision: Vec<String> = [
        "vision_tower.",
        "vision_model.",
        "model.vision_model.",
        "multi_modal_projector.",
        "model.vision_tower.",
        "model.multi_modal_projector.",
        "model.visual.",
        "visual.",
        "mtp.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut sub = P {
        cfg: Cfg::new(arch.to_string(), text, "text_config"),
        notes: vec![],
        unsure: vec![],
        reference: p.reference.clone(),
        layouts: Layouts::default(),
        ignored_prefixes: vec![],
    };
    sub.cfg.inert(&["model_type"]);
    let mut spec = match tmt {
        "gemma3_text" => dense::gemma3_text(&mut sub, prefix, lm_head, aliases)?,
        "llama4_text" => {
            let mut s = moe::llama4_text(&mut sub, prefix, lm_head)?;
            s.hf.prefix_aliases = aliases;
            s
        }
        "qwen3_5_text" => hybrid::qwen3_5_text(&mut sub, false, prefix, lm_head, aliases)?,
        "qwen3_5_moe_text" => hybrid::qwen3_5_text(&mut sub, true, prefix, lm_head, aliases)?,
        "llama" | "mistral" | "ministral3" | "qwen2" | "qwen2_vl_text" | "qwen2_5_vl_text" => {
            let flavor = match tmt {
                "llama" => Flavor::Llama,
                "mistral" | "ministral3" => Flavor::Mistral,
                _ => Flavor::Qwen2,
            };
            let mut s = dense::llama_with_prefix(&mut sub, flavor, prefix, lm_head)?;
            s.hf.prefix_aliases = aliases;
            s
        }
        other => return Err(LowerError::not_lowerable(format!("{arch}: text decoder model_type `{other}` is not modelled"))),
    };
    sub.cfg.finish()?;
    p.cfg.inert(&["vocab_size"]);
    // transformers 5 ties through the COMPOSITE config (`PreTrainedModel` reads
    // `self.config.tie_word_embeddings` on the VLM), whose class defaults differ: Gemma-3 and
    // Mistral-3 default to tied, LLaVA to untied OR the text config's flag, Qwen3.5 to untied.
    let tied = match arch {
        "LlavaForConditionalGeneration" => root_tie.unwrap_or(false) || spec.head.tied,
        "Gemma3ForConditionalGeneration" | "Mistral3ForConditionalGeneration" => root_tie.unwrap_or(true),
        _ => root_tie.unwrap_or(false),
    };
    if tied != spec.head.tied {
        spec.notes.push(format!("the VLM config's tie_word_embeddings ({tied}) overrides the text config's ({})", spec.head.tied));
        spec.head.tied = tied;
        let head = if tied { spec.hf.names["embed"].clone() } else { lm_head.to_string() };
        spec.hf.names.insert("lm_head".into(), head);
    }
    spec.architecture = arch.to_string();
    spec.hf.ignored_prefixes.extend(vision);
    spec.notes.push("text decoder only: vision tower, projector and image tokens are not lowered".into());
    spec.notes.extend(sub.notes);
    if let Confidence::Unsure(u) = &mut spec.confidence {
        u.extend(sub.unsure);
    } else if !sub.unsure.is_empty() {
        spec.confidence = Confidence::Unsure(sub.unsure);
    }
    Ok(spec)
}

// ───────────────────────────── shared helpers ─────────────────────────────

pub(crate) struct P<'a> {
    pub cfg: Cfg<'a>,
    pub notes: Vec<String>,
    pub unsure: Vec<String>,
    pub reference: Reference,
    /// Fused-weight layouts of this checkpoint (HF storage, see `HfStorage`).
    pub layouts: Layouts,
    /// Tensor-name prefixes the lowering does not read by design (see `HfStorage`).
    pub ignored_prefixes: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Layouts {
    pub qkv: QkvLayout,
    pub mlp: MlpLayout,
    pub experts: MlpLayout,
    pub gdn: GdnLayout,
}

impl Default for Layouts {
    fn default() -> Self {
        Layouts { qkv: QkvLayout::Separate, mlp: MlpLayout::Separate, experts: MlpLayout::Separate, gdn: GdnLayout::FusedPerKeyHead }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flavor {
    Llama,
    Mistral,
    Qwen2,
    Qwen3,
    Granite,
    MiniCpm,
    SmolLm3,
}

impl P<'_> {
    pub fn arch(&self) -> String {
        self.cfg.arch.clone()
    }

    /// Read an activation name; refuse names this lowerer does not implement.
    pub fn act(&self, key: &str, default: &str) -> Result<Act> {
        let name = self.cfg.str_or(key, default)?;
        Act::from_hf(&name).ok_or_else(|| LowerError::not_lowerable(format!("{}: activation `{name}` is not modelled", self.cfg.arch)))
    }

    /// `layer_types` if present (validated), else the arch's default pattern.
    pub fn layer_types(&self, n: usize, allowed: &[&str], default: impl Fn(usize) -> &'static str) -> Result<Vec<String>> {
        match self.cfg.opt_str_list("layer_types")? {
            Some(l) => {
                if l.len() != n {
                    return Err(LowerError::bad(format!("{}: layer_types has {} entries for {n} layers", self.cfg.arch, l.len())));
                }
                if let Some(bad) = l.iter().find(|t| !allowed.contains(&t.as_str())) {
                    return Err(LowerError::not_lowerable(format!("{}: layer type `{bad}` is not modelled", self.cfg.arch)));
                }
                Ok(l)
            }
            None => Ok((0..n).map(|i| default(i).to_string()).collect()),
        }
    }

    /// A rotary spec: frequencies from the config's rope fields.
    #[allow(clippy::too_many_arguments)]
    pub fn rope(
        &self,
        rotary_dim: usize,
        style: RopeStyle,
        theta_default: Option<f64>,
        layer_type: Option<&str>,
        partial: f64,
        max_pos: Option<usize>,
        top_orig: Option<usize>,
    ) -> Result<RopeSpec> {
        Ok(self.rope_q_scaled(rotary_dim, style, theta_default, layer_type, partial, max_pos, top_orig, false)?.0)
    }

    /// [`P::rope`], and the query temperature a `llama_4_scaling_beta` among the rope parameters
    /// asks for (Ministral-3: `q ·= 1 + β·ln(1 + ⌊p / original_max_position_embeddings⌋)`), which
    /// only an architecture that applies it may carry (`q_scaled`) — elsewhere it is refused, never
    /// dropped.
    #[allow(clippy::too_many_arguments)]
    pub fn rope_q_scaled(
        &self,
        rotary_dim: usize,
        style: RopeStyle,
        theta_default: Option<f64>,
        layer_type: Option<&str>,
        partial: f64,
        max_pos: Option<usize>,
        top_orig: Option<usize>,
        q_scaled: bool,
    ) -> Result<(RopeSpec, Option<QTemperature>)> {
        crate::rope::rope_spec_from_config(&self.cfg, rotary_dim, style, theta_default, layer_type, partial, max_pos, top_orig, q_scaled)
    }

    pub fn finish_spec(&mut self, s: SpecParts) -> ArchSpec {
        let confidence = if self.unsure.is_empty() { Confidence::Known } else { Confidence::Unsure(std::mem::take(&mut self.unsure)) };
        ArchSpec {
            architecture: self.cfg.arch.clone(),
            model_type: s.model_type.to_string(),
            families: s.families.into_iter().map(String::from).collect(),
            reference: self.reference.clone(),
            confidence,
            vocab_size: s.vocab,
            hidden_size: s.hidden,
            max_position_embeddings: s.max_pos,
            embedding: s.embedding,
            layers: s.layers,
            final_norm: s.final_norm,
            head: s.head,
            hyper: None,
            altup: None,
            mhc: None,
            output: OutputSpec::Logits,
            adapter: None,
            hf: HfStorage {
                names: s.names,
                prefix_aliases: s.prefix_aliases,
                conv1d_weights: s.conv1d,
                qkv: self.layouts.qkv,
                mlp: self.layouts.mlp,
                experts: self.layouts.experts,
                gdn: self.layouts.gdn,
                ignored_prefixes: std::mem::take(&mut self.ignored_prefixes),
                quant: None,
                table_shards: 1,
                weights: Default::default(),
            },
            notes: std::mem::take(&mut self.notes),
            prefix_lm: false,
        }
    }
}

pub(crate) struct SpecParts {
    pub model_type: &'static str,
    pub families: Vec<&'static str>,
    pub vocab: usize,
    pub hidden: usize,
    pub max_pos: Option<usize>,
    pub embedding: EmbeddingSpec,
    pub layers: Vec<LayerSpec>,
    pub final_norm: Option<NormSpec>,
    pub head: HeadSpec,
    pub names: BTreeMap<String, String>,
    pub prefix_aliases: Vec<(String, String)>,
    pub conv1d: bool,
}

pub(crate) fn names(pairs: &[(&str, String)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// The Llama naming scheme under `model` (e.g. `model.`).
pub(crate) fn llama_names(model: &str, lm_head: &str) -> BTreeMap<String, String> {
    let l = format!("{model}layers.{{L}}.");
    names(&[
        ("embed", format!("{model}embed_tokens")),
        ("final_norm", format!("{model}norm")),
        ("lm_head", lm_head.to_string()),
        ("norm.mix", format!("{l}input_layernorm")),
        ("norm.ffn", format!("{l}post_attention_layernorm")),
        ("attn.q", format!("{l}self_attn.q_proj")),
        ("attn.k", format!("{l}self_attn.k_proj")),
        ("attn.v", format!("{l}self_attn.v_proj")),
        ("attn.o", format!("{l}self_attn.o_proj")),
        ("attn.q_norm", format!("{l}self_attn.q_norm")),
        ("attn.k_norm", format!("{l}self_attn.k_norm")),
        ("mlp.gate", format!("{l}mlp.gate_proj")),
        ("mlp.up", format!("{l}mlp.up_proj")),
        ("mlp.down", format!("{l}mlp.down_proj")),
    ])
}

pub(crate) fn plain_embedding(dim: usize) -> EmbeddingSpec {
    EmbeddingSpec { dim, scale: 1.0, positions: None, norm: None, proj_in: false, proj_after_norm: false, proj_in_bias: false, type_rows: None, rel_bias: None, disentangled: None }
}

pub(crate) fn plain_head(tied: bool) -> HeadSpec {
    HeadSpec { tied, bias: false, pre_scale: 1.0, proj_out: false, logit_scale: 1.0, softcap: None, transform: None }
}

pub(crate) fn pre_norm(n: NormSpec) -> Residual {
    Residual::Sequential { pre_mixer: Some(n), post_mixer: None, pre_ffn: Some(n), post_ffn: None, multiplier: 1.0 }
}

pub(crate) fn gated_mlp(intermediate: usize, act: Act, bias: bool) -> MlpSpec {
    MlpSpec { intermediate, act, gated: true, glu: Glu::Standard, up_bias: bias, down_bias: bias, inner_norm: None, name: None, sparsity: None }
}

pub(crate) fn plain_mlp(intermediate: usize, act: Act, bias: bool) -> MlpSpec {
    MlpSpec { intermediate, act, gated: false, glu: Glu::Standard, up_bias: bias, down_bias: bias, inner_norm: None, name: None, sparsity: None }
}

/// Heads / kv heads / head_dim with the usual defaults and divisibility checks.
pub(crate) fn heads(
    p: &P,
    hidden: usize,
    heads_key: &str,
    heads_default: usize,
    kv_default: Option<usize>,
    hd_default: Option<usize>,
) -> Result<(usize, usize, usize)> {
    let h = p.cfg.usize_or(heads_key, heads_default)?;
    let kv = match p.cfg.opt_usize("num_key_value_heads")? {
        Some(k) => k,
        None => kv_default.unwrap_or(h),
    };
    let hd = match p.cfg.opt_usize("head_dim")?.or(hd_default) {
        Some(d) => d,
        None => {
            if h == 0 || !hidden.is_multiple_of(h) {
                return Err(LowerError::bad(format!("{}: hidden {hidden} not divisible by {h} heads", p.cfg.arch)));
            }
            hidden / h
        }
    };
    if h == 0 || kv == 0 || h % kv != 0 {
        return Err(LowerError::bad(format!("{}: {h} heads are not a multiple of {kv} kv heads", p.cfg.arch)));
    }
    Ok((h, kv, hd))
}

pub(crate) fn attn(h: usize, kv: usize, hd: usize, position: Position, bias: (bool, bool)) -> AttnSpec {
    AttnSpec {
        heads: h,
        kv_heads: kv,
        head_dim: hd,
        v_head_dim: hd,
        q_bias: bias.0,
        k_bias: bias.0,
        v_bias: bias.0,
        o_bias: bias.1,
        qk_norm: None,
        qk_norm_after_rope: false,
        o_norm: None,
        clip_qkv: None,
        position,
        scale: 1.0 / (hd as f64).sqrt(),
        softcap: None,
        window: None,
        sinks: false,
        output_gate: false,
        chunk: None,
        q_temperature: None,
        v_norm: None,
        v_from_k: false,
        param_prefix: None,
        kv_share: None,
        sparse: None,
        gate: None,
        v_scale: 1.0,
    }
}

pub(crate) fn window_for(t: &str, sw: Option<usize>) -> Option<usize> {
    if t == "sliding_attention" { sw } else { None }
}

