//! **`config.json` → [`ArchSpec`]**: the entry points of the HF schema reader.
//!
//! Which keys a model has, what its defaults are and how its tensors are named is **data**: an
//! adapter file (`adapters/*.json`, [`crate::adapter`]) read by the generic reader
//! ([`crate::hf_schema`]). This module keeps what is not per-family: the lenient JSON reader
//! ([`sanitize_json`]), the two parse entry points, the remote-code module name and the attachment
//! of a pre-quantised checkpoint's integers.
//!
//! The per-architecture Rust parsers this crate had before the adapters exist only behind the
//! `legacy-oracle` cargo feature ([`legacy_oracle`]): the differential oracle of `tests/adapters.rs`
//! that held every adapter to them. No build without the feature carries per-family code.

#[cfg(feature = "legacy-oracle")]
pub mod legacy_oracle;
#[cfg(feature = "legacy-oracle")]
pub use legacy_oracle::parse_legacy;

use crate::error::{LowerError, Result};
use crate::spec::*;
use serde_json::{Map, Value};

/// Replace non-standard JSON number tokens Python writes (`Infinity`, `-Infinity`, `NaN`) outside
/// strings: `Mamba2Config.time_step_limit` serialises as `[0.0, Infinity]`.
pub fn sanitize_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let b = text.as_bytes();
    let (mut i, mut in_str, mut esc) = (0usize, false, false);
    while i < b.len() {
        let c = b[i] as char;
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("-Infinity") {
            out.push_str("-1.7976931348623157e308");
            i += 9;
        } else if rest.starts_with("Infinity") {
            out.push_str("1.7976931348623157e308");
            i += 8;
        } else if rest.starts_with("NaN") {
            out.push_str("null");
            i += 3;
        } else {
            // Copy one UTF-8 character.
            let ch = rest.chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

pub fn parse_config_str(text: &str) -> Result<ArchSpec> {
    parse_config_str_with(text, crate::quantfmt::QuantRegistry::builtin())
}

/// [`parse_config_str`] with the quant formats of `reg` (built-ins plus the descriptors a model needs).
pub fn parse_config_str_with(text: &str, reg: &crate::quantfmt::QuantRegistry) -> Result<ArchSpec> {
    let v: Value = serde_json::from_str(&sanitize_json(text)).map_err(|e| LowerError::bad(format!("config.json is not JSON: {e}")))?;
    parse_config_with(&v, reg)
}

/// [`parse_config_str_with`] choosing the adapter by `read` (a user-supplied adapter file, a built-in
/// by id, or none): how a model written for as data is lowered, end to end, with no code change.
pub fn parse_config_str_read(text: &str, read: &crate::hf_schema::ReadOptions, reg: &crate::quantfmt::QuantRegistry) -> Result<ArchSpec> {
    let v: Value = serde_json::from_str(&sanitize_json(text)).map_err(|e| LowerError::bad(format!("config.json is not JSON: {e}")))?;
    crate::hf_schema::read_model_with(&v, None, read, reg).map(|r| r.spec).map_err(|f| f.error)
}

pub(crate) fn remote_module(auto_map: &Map<String, Value>) -> Option<String> {
    let v = auto_map.get("AutoModelForCausalLM").or_else(|| auto_map.get("AutoModel"))?;
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.first()?.as_str()?.to_string(),
        _ => return None,
    };
    Some(s.rsplit("--").next().unwrap_or(&s).to_string())
}

/// A configuration through the schema reader ([`crate::hf_schema::read_model`]) with no tensor index
/// and the default adapter choice.
pub fn parse_config(v: &Value) -> Result<ArchSpec> {
    parse_config_with(v, crate::quantfmt::QuantRegistry::builtin())
}

/// [`parse_config`] with the quant formats of `reg` (built-ins plus the descriptors a model needs).
pub fn parse_config_with(v: &Value, reg: &crate::quantfmt::QuantRegistry) -> Result<ArchSpec> {
    crate::hf_schema::read_model_with(v, None, &crate::hf_schema::ReadOptions::default(), reg).map(|r| r.spec).map_err(|f| f.error)
}

/// Attach a pre-quantised checkpoint's config to a parsed spec: dense attention + MLP decoders,
/// the checkpoint's projections read as stored integers (`crate::prequant`).
pub(crate) fn attach_quant(spec: &mut ArchSpec, q: crate::prequant::QuantConfig) -> Result<()> {
    let arch = spec.architecture.clone();
    if !matches!(spec.output, OutputSpec::Logits) || spec.hf.conv1d_weights || spec.adapter.is_some() {
        return Err(LowerError::not_lowerable(format!(
            "{arch}: a pre-quantised checkpoint of this kind (only decoders with nn.Linear projections)"
        )));
    }
    let visual = |k: &str| k.contains("vision") || k.contains("visual") || k.contains("projector");
    if spec.hf.names.keys().any(|k| visual(k)) || spec.hf.ignored_prefixes.iter().any(|k| visual(k)) {
        return Err(LowerError::not_lowerable(format!("{arch}: a pre-quantised multimodal checkpoint")));
    }
    // A format that decodes to floats (FP8) hands the lowerer an ordinary float weight, whatever the
    // layer is; the integers-lowering below is for the layers `crate::lower::qlinear` knows.
    let integers = q.fmt.is_integers();
    for (l, ls) in spec.layers.iter().enumerate() {
        if integers && (!matches!(ls.mixer, Mixer::Attention(_) | Mixer::GatedDeltaNet(_)) || !matches!(ls.ffn, Ffn::Mlp(_) | Ffn::Moe(_))) {
            return Err(LowerError::not_lowerable(format!(
                "{arch}: layer {l} of a pre-quantised checkpoint is not attention or gated delta + MLP or experts (latent attention and the other recurrent mixers are not lowered from their integers yet)"
            )));
        }
        if let Ffn::Moe(m) = &ls.ffn
            && spec.hf.experts != crate::spec::MlpLayout::Separate
        {
            let _ = m;
            return Err(LowerError::not_lowerable(format!("{arch}: pre-quantised experts in a fused layout")));
        }
    }
    spec.notes.push(if integers {
        format!("pre-quantised checkpoint ({}): projections lowered from the stored integers", q.fmt.label())
    } else {
        format!("pre-quantised checkpoint ({}): projections decoded to float32 and lowered on the ordinary W8 path", q.fmt.label())
    });
    spec.hf.quant = Some(q);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_infinity_is_sanitised_outside_strings_only() {
        let s = r#"{"a": [0.0, Infinity], "b": "Infinity NaN", "c": -Infinity, "d": NaN}"#;
        let v: Value = serde_json::from_str(&sanitize_json(s)).unwrap();
        assert!(v["a"][1].as_f64().unwrap() > 1e300);
        assert_eq!(v["b"], "Infinity NaN");
        assert!(v["c"].as_f64().unwrap() < -1e300);
        assert!(v["d"].is_null());
    }

    #[test]
    fn refusals_happen_before_any_family_parser() {
        let e = parse_config(&serde_json::json!({"architectures": ["T5ForConditionalGeneration"], "is_encoder_decoder": true}))
            .unwrap_err();
        assert!(matches!(e, LowerError::NotLowerable(ref s) if s.contains("encoder–decoder")), "{e}");
        let e = parse_config(&serde_json::json!({"hidden_size": 8})).unwrap_err();
        assert!(matches!(e, LowerError::NotLowerable(ref s) if s.contains("architectures")), "{e}");
        // GPTQ and AWQ are read (crate::prequant); every other method is refused up front.
        let e = parse_config(
            &serde_json::json!({"architectures": ["LlamaForCausalLM"], "quantization_config": {"quant_method": "bitsandbytes"}}),
        )
        .unwrap_err();
        assert!(matches!(e, LowerError::NotLowerable(ref s) if s.contains("bitsandbytes")), "{e}");
        let e = parse_config(
            &serde_json::json!({"architectures": ["LlamaForCausalLM"], "quantization_config": {"quant_method": "gptq", "bits": 3}}),
        )
        .unwrap_err();
        assert!(matches!(e, LowerError::NotLowerable(ref s) if s.contains("GPTQ 3-bit")), "{e}");
        let e = parse_config(&serde_json::json!({
            "architectures": ["LlamaForCausalLM"],
            "auto_map": {"AutoModelForCausalLM": "modeling_custom.CustomLlama"}
        }))
        .unwrap_err();
        assert!(matches!(e, LowerError::NotLowerable(ref s) if s.contains("modeling_custom")), "{e}");
        let e = parse_config(&serde_json::json!({"architectures": ["InternLM2ForCausalLM"]})).unwrap_err();
        assert!(matches!(e, LowerError::NotLowerable(ref s) if s.contains("remote code")), "{e}");
        let e = parse_config(&serde_json::json!({"architectures": ["BertForMaskedLM"]})).unwrap_err();
        assert!(matches!(e, LowerError::NotLowerable(_)), "{e}");
    }
}
