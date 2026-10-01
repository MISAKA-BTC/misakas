//! **What an artifact says of itself**: the provenance record `palw-tir-convert` writes into the
//! container (`misaka.palw.runtime-pack.v1`'s companion — a pack pins the same facts by hash).
//! `misaka model inspect` and `palw-class inspect` print it, so an operator sees the feature scope
//! (what the class does not compute), the adapter and quant descriptors it was read with, and the
//! math and calibration it was built under, without the pack at hand.

use serde_json::Value;

/// The provenance facts of a container's `meta` JSON that are worth a line each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Provenance {
    pub lines: Vec<String>,
    /// The structured subset (`frontend`, `scope`, `quant`, `math`, `converter`, `calibration`, `policy`),
    /// for `--json`.
    pub json: Value,
}

fn short(v: &Value) -> String {
    v.as_str().map(|s| s.chars().take(16).collect()).unwrap_or_default()
}

pub fn provenance_of(meta: &str) -> Provenance {
    let Ok(m) = serde_json::from_str::<Value>(meta) else { return Provenance::default() };
    if !m.is_object() {
        return Provenance::default();
    }
    let mut lines = Vec::new();
    if let Some(f) = m.get("frontend") {
        let a = &f["adapter"];
        let adapter = match a["kind"].as_str() {
            Some("built-in") | Some("user-file") => format!("{} adapter `{}` {}", a["kind"].as_str().unwrap_or(""), a["id"].as_str().unwrap_or(""), short(&a["hash"])),
            _ => "no adapter (the standard decoder template)".to_string(),
        };
        lines.push(format!("frontend         level {}, {}, spec {}", f["level"].as_str().unwrap_or("?"), adapter, short(&f["spec_digest"])));
    }
    if let Some(s) = m.get("scope") {
        let left: Vec<&str> = s["excluded"].as_array().map(|a| a.iter().filter_map(|e| e["what"].as_str()).collect()).unwrap_or_default();
        lines.push(format!(
            "scope            {} → {}; {}",
            s["input"].as_str().unwrap_or("?"),
            s["output"].as_str().unwrap_or("?"),
            if left.is_empty() { "nothing left out".to_string() } else { format!("NOT computed: {}", left.join(", ")) }
        ));
        if s["text_only"] == true {
            lines.push("                 (this class is the model's text decoder only: a prompt cannot carry an image or audio)".to_string());
        }
    }
    if let Some(q) = m["quant"]["descriptors"].as_array().filter(|a| !a.is_empty()) {
        lines.push(format!("quant formats    {}", q.iter().map(|d| format!("{} {}", d["name"].as_str().unwrap_or(""), short(&d["digest"]))).collect::<Vec<_>>().join(", ")));
    }
    if let Some(math) = m.get("math") {
        let plat = math["platform"].as_str().map(|p| format!(" (built on {p}: only that platform rebuilds it)")).unwrap_or_default();
        lines.push(format!("math             {}{}", math["mode"].as_str().unwrap_or("platform libm (legacy, before libm-v1)"), plat));
    } else if m.get("converter").is_some() {
        lines.push("math             platform libm (built before libm-v1: rebuildable on the platform that built it)".to_string());
    }
    if let Some(sc) = m["logits_scale"].as_f64() {
        match m["logits_convention"].as_str() {
            Some("q24-natural-v1") => lines.push("logits           q24-natural-v1: code / 2^24 are natural-log logits (the chain's convention)".to_string()),
            _ => lines.push(format!("logits           codes × {sc:e} are the logits; legacy greedy-only convention: the chain reads the codes' order, not their units")),
        }
    }
    if let Some(c) = m.get("converter").and_then(Value::as_str) {
        lines.push(format!("converter        {c}"));
    }
    if let Some(c) = m.get("calibration") {
        lines.push(format!("calibration      statistics {}", short(&c["digest"])));
    }
    let keep = ["frontend", "scope", "quant", "math", "converter", "calibration", "policy", "logits_scale"];
    let json = Value::Object(m.as_object().map(|o| o.iter().filter(|(k, _)| keep.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default());
    Provenance { lines, json }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_converters_record_prints_scope_adapter_descriptors_math_and_logits() {
        let meta = serde_json::json!({
            "architecture": "LlavaForConditionalGeneration",
            "logits_scale": 0.0012,
            "frontend": { "adapter": { "kind": "built-in", "id": "vlm-llama", "hash": "ab".repeat(64) }, "level": "B", "spec_digest": "cd".repeat(32) },
            "scope": { "input": "token ids", "output": "next-token logits", "text_only": true, "excluded": [ { "what": "vision tower" }, { "what": "multimodal projector" } ] },
            "quant": { "descriptors": [ { "name": "Q4_K", "digest": "ef".repeat(32) } ] },
            "math": { "mode": "libm-v1" },
            "converter": "palw-tir-convert 1.1.0",
            "calibration": { "schema": "misaka.palw.calib-stats.v1", "digest": "12".repeat(32) },
        })
        .to_string();
        let p = provenance_of(&meta);
        let all = p.lines.join("\n");
        assert!(all.contains("level B, built-in adapter `vlm-llama` abababababababab"), "{all}");
        assert!(all.contains("NOT computed: vision tower, multimodal projector") && all.contains("text decoder only"), "{all}");
        assert!(all.contains("Q4_K efefefefefefefef") && all.contains("math             libm-v1") && all.contains("legacy greedy-only"), "{all}");
        assert_eq!(p.json["scope"]["text_only"], true);
        // An artifact built before the record existed prints nothing rather than guessing.
        assert!(provenance_of("{}").lines.is_empty() && provenance_of("not json").lines.is_empty());
        // One that says only that a converter made it was built on the platform's libm.
        assert!(provenance_of(r#"{"converter": "palw-tir-convert 1.1.0"}"#).lines.iter().any(|l| l.contains("platform libm")));
    }
}
