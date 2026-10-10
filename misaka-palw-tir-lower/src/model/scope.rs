//! **The feature scope of a class: what the model has that the class does not compute**
//! (RFC-0002 Part II, requirement 4).
//!
//! A registered class is one function — token ids in, next-token logits (or a pooled embedding) out.
//! A model that also takes images or audio, or proposes several tokens at once, has parts this class
//! leaves out. The scope names them, with the evidence (configuration keys, tensor-name prefixes with
//! their counts and bytes, files beside the checkpoint such as a GGUF's `mmproj`) and the reason, so
//! that preflight, the runtime pack and `misaka model inspect` never present a text-only class as the
//! whole model — and never as a partly multimodal one: the class either computes a part entirely or
//! says it does not.
//!
//! The rules are data (`scope/rules.v1.json`, `misaka.palw.scope-rules.v1`): which configuration keys,
//! tensor prefixes and file names are evidence of which part. Nothing here selects code by model name.

use crate::hf_schema::TensorIndex;
use crate::spec::{ModelSpec, OutputSpec};
use serde::Serialize;
use serde_json::Value;
use std::sync::OnceLock;

/// Schema id of a serialised [`FeatureScope`].
pub const SCOPE_SCHEMA_V1: &str = "misaka.palw.feature-scope.v1";
/// Schema id of the rules file.
pub const SCOPE_RULES_SCHEMA_V1: &str = "misaka.palw.scope-rules.v1";

const RULES_JSON: &str = include_str!("../../scope/rules.v1.json");

/// One part of the model the class does not compute.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Excluded {
    /// `vision`, `projector`, `audio`, `mtp`, `draft`.
    pub kind: String,
    pub what: String,
    pub why: String,
    /// Whether the part is a modality (an input the class cannot take), not a head the class can do without.
    pub modal: bool,
    /// What showed it: `config.<key>`, `tensors <prefix>*`, `file <name>`.
    pub evidence: Vec<String>,
    /// Tensors matching the rule's prefixes, and their bytes (when a tensor index with shapes was read).
    pub tensors: usize,
    pub bytes: Option<u64>,
}

/// What a class computes and what it leaves out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FeatureScope {
    pub schema: &'static str,
    /// `text-generation` (a causal language model) or `text-embedding` (an encoder).
    pub task: String,
    pub input: String,
    pub output: String,
    /// The model has a modality this class does not take (images, audio): the class is its text decoder only.
    pub text_only: bool,
    pub excluded: Vec<Excluded>,
}

impl FeatureScope {
    /// The modalities left out, by name (`vision`, `audio`).
    pub fn modalities_left_out(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.excluded.iter().filter(|e| e.modal && e.kind != "projector").map(|e| e.kind.as_str()).collect();
        v.dedup();
        v
    }

    /// The one line a preflight prints.
    pub fn headline(&self) -> String {
        if self.excluded.is_empty() {
            format!("{}: {} → {}; nothing left out", self.task, self.input, self.output)
        } else {
            format!(
                "{}: {} → {}; NOT computed: {}",
                self.task,
                self.input,
                self.output,
                self.excluded.iter().map(|e| e.what.as_str()).collect::<Vec<_>>().join(", ")
            )
        }
    }
}

#[derive(Clone, Debug)]
struct Rule {
    kind: String,
    what: String,
    why: String,
    modal: bool,
    config_keys: Vec<String>,
    tensor_prefixes: Vec<String>,
    tensor_contains: Vec<String>,
    files: Vec<String>,
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn rules() -> &'static [Rule] {
    static R: OnceLock<Vec<Rule>> = OnceLock::new();
    R.get_or_init(|| {
        let v: Value = serde_json::from_str(RULES_JSON).expect("the built-in scope rules are JSON");
        assert_eq!(v["schema"], SCOPE_RULES_SCHEMA_V1, "the built-in scope rules' schema");
        v["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .map(|r| Rule {
                kind: r["kind"].as_str().unwrap_or_default().to_string(),
                what: r["what"].as_str().unwrap_or_default().to_string(),
                why: r["why"].as_str().unwrap_or_default().to_string(),
                modal: r["modal"].as_bool().unwrap_or(false),
                config_keys: strings(r, "config_keys"),
                tensor_prefixes: strings(r, "tensor_prefixes"),
                tensor_contains: strings(r, "tensor_contains"),
                files: strings(r, "files"),
            })
            .collect()
    })
}

/// `*` matches any run of characters; everything else is literal; case-insensitive.
fn glob(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.to_lowercase().chars().collect(), name.to_lowercase().chars().collect());
    fn go(p: &[char], n: &[char]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some(('*', rest)) => (0..=n.len()).any(|k| go(rest, &n[k..])),
            Some((c, rest)) => n.first() == Some(c) && go(rest, &n[1..]),
        }
    }
    go(&p, &n)
}

fn dtype_bytes(d: &str) -> Option<u64> {
    Some(match d {
        "F64" | "I64" | "U64" => 8,
        "F32" | "I32" | "U32" => 4,
        "F16" | "BF16" | "I16" | "U16" => 2,
        "I8" | "U8" | "BOOL" | "F8_E4M3" | "F8_E5M2" => 1,
        _ => return None,
    })
}

/// Whether a configuration carries `key` (a non-null value, at the root).
fn has_key(config: &Value, key: &str) -> bool {
    config.get(key).is_some_and(|v| !v.is_null())
}

/// **The scope of a model.** `config` is the Hugging Face configuration (or the GGUF's, synthesised);
/// `spec` the lowered description when there is one (it says whether the class is a decoder or an
/// encoder); `tensors` the checkpoint's tensor names (and shapes: then bytes are counted); `files` the
/// names of the files beside the checkpoint.
pub fn scope_of(config: &Value, spec: Option<&ModelSpec>, tensors: Option<&TensorIndex>, files: &[String]) -> FeatureScope {
    let (task, input, output) = match spec.map(|s| &s.output) {
        Some(OutputSpec::Embedding { .. }) => ("text-embedding", "token ids", "a pooled sentence embedding"),
        Some(OutputSpec::Classify { .. }) => ("text-classification", "token ids", "one logit per label"),
        // Two logits per token are a span head's start and end (`…ForQuestionAnswering`); any other width a token classifier's labels.
        Some(OutputSpec::TokenLogits { labels: 2, .. }) if spec.is_some_and(|s| s.architecture.ends_with("ForQuestionAnswering")) => {
            ("question-answering", "token ids (question ‖ separator ‖ context)", "a start and an end logit per token")
        }
        Some(OutputSpec::TokenLogits { .. }) => ("token-classification", "token ids", "one logit per label per token"),
        Some(OutputSpec::MaskedLm) => ("fill-mask", "token ids (one of them the mask token)", "vocabulary logits per token"),
        _ => ("text-generation", "token ids", "next-token logits"),
    };
    let mut excluded = Vec::new();
    for r in rules() {
        let mut evidence = Vec::new();
        for k in &r.config_keys {
            if has_key(config, k) {
                evidence.push(format!("config.{k}"));
            }
        }
        let (mut n, mut bytes, mut sized) = (0usize, 0u64, true);
        let mut by_prefix: std::collections::BTreeMap<String, usize> = Default::default();
        if let Some(t) = tensors {
            for (name, e) in &t.tensors {
                let hit = r.tensor_prefixes.iter().find(|p| name.starts_with(p.as_str())).or_else(|| r.tensor_contains.iter().find(|p| name.contains(p.as_str())));
                if let Some(p) = hit {
                    n += 1;
                    *by_prefix.entry(p.clone()).or_default() += 1;
                    match (e.shape.as_ref(), dtype_bytes(&e.dtype)) {
                        (Some(s), Some(b)) => bytes += s.iter().product::<usize>() as u64 * b,
                        _ => sized = false,
                    }
                }
            }
        }
        for (p, c) in &by_prefix {
            evidence.push(format!("tensors {p}* ({c})"));
        }
        for f in files {
            if r.files.iter().any(|g| glob(g, f)) {
                evidence.push(format!("file {f}"));
            }
        }
        if !evidence.is_empty() {
            excluded.push(Excluded {
                kind: r.kind.clone(),
                what: r.what.clone(),
                why: r.why.clone(),
                modal: r.modal,
                evidence,
                tensors: n,
                bytes: (tensors.is_some() && sized).then_some(bytes),
            });
        }
    }
    let text_only = excluded.iter().any(|e| e.modal && e.kind != "projector");
    FeatureScope { schema: SCOPE_SCHEMA_V1, task: task.into(), input: input.into(), output: output.into(), text_only, excluded }
}

/// The names of the files in the directory a checkpoint lives in (a GGUF's `mmproj` sits beside it).
pub fn sibling_files(path: &std::path::Path) -> Vec<String> {
    let dir = if path.is_dir() { Some(path) } else { path.parent() };
    let mut v: Vec<String> = dir
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_plain_decoder_leaves_nothing_out() {
        let s = scope_of(&json!({"architectures": ["LlamaForCausalLM"], "hidden_size": 8}), None, None, &[]);
        assert!(s.excluded.is_empty() && !s.text_only);
        assert_eq!((s.task.as_str(), s.output.as_str()), ("text-generation", "next-token logits"));
        assert!(s.headline().contains("nothing left out"));
    }

    #[test]
    fn a_vision_language_model_is_text_only_and_says_what_it_does_not_compute() {
        let cfg = json!({"architectures": ["LlavaForConditionalGeneration"], "vision_config": {}, "image_token_index": 32000, "projector_hidden_act": "gelu"});
        let t = TensorIndex::from_shapes([
            ("language_model.model.embed_tokens.weight", vec![8, 4]),
            ("vision_tower.vision_model.encoder.layers.0.mlp.fc1.weight", vec![16, 4]),
            ("vision_tower.vision_model.embeddings.class_embedding", vec![4]),
            ("multi_modal_projector.linear_1.weight", vec![4, 4]),
        ]);
        let s = scope_of(&cfg, None, Some(&t), &[]);
        assert!(s.text_only);
        assert_eq!(s.modalities_left_out(), vec!["vision"]);
        let v = s.excluded.iter().find(|e| e.kind == "vision").expect("vision");
        assert!(v.evidence.contains(&"config.vision_config".to_string()) && v.evidence.iter().any(|e| e.starts_with("tensors vision_tower.*")));
        assert_eq!(v.tensors, 2);
        let p = s.excluded.iter().find(|e| e.kind == "projector").expect("projector");
        assert_eq!(p.tensors, 1);
        assert!(s.headline().contains("NOT computed: vision tower, multimodal projector"));
    }

    #[test]
    fn mtp_heads_are_left_out_without_making_the_class_text_only() {
        let cfg = json!({"architectures": ["DeepseekV3ForCausalLM"], "num_nextn_predict_layers": 1});
        let t = TensorIndex::from_shapes([("model.layers.61.eh_proj.weight", vec![2, 2]), ("mtp.0.weight", vec![2, 2])]);
        let s = scope_of(&cfg, None, Some(&t), &[]);
        assert!(!s.text_only);
        assert_eq!(s.excluded.len(), 1);
        assert_eq!(s.excluded[0].kind, "mtp");
        assert!(!s.excluded[0].modal);
    }

    #[test]
    fn a_gguf_with_an_mmproj_beside_it_is_text_only() {
        let cfg = json!({"architectures": ["Qwen3ForCausalLM"]});
        let files = vec!["Model-27B-Q4_K_M.gguf".to_string(), "mmproj-Model-27B-F16.gguf".to_string(), "README.md".to_string()];
        let s = scope_of(&cfg, None, None, &files);
        assert!(s.text_only);
        let v = &s.excluded[0];
        assert_eq!(v.kind, "vision");
        assert_eq!(v.evidence, vec!["file mmproj-Model-27B-F16.gguf".to_string()]);
        assert_eq!(v.bytes, None, "no tensor index: no byte count");
    }

    #[test]
    fn the_glob_is_a_case_insensitive_star_and_nothing_else() {
        assert!(glob("*mmproj*.gguf", "MMProj-x.GGUF") && glob("a*c", "abbbc") && !glob("a*c", "abbbd") && !glob("*.gguf", "x.gguf.bak"));
    }

    #[test]
    fn bytes_are_counted_from_shapes_and_dtypes() {
        use crate::hf_schema::TensorEntry;
        let mut t = TensorIndex::default();
        t.tensors.insert("visual.blocks.0.attn.qkv.weight".into(), TensorEntry { dtype: "BF16".into(), shape: Some(vec![10, 20]) });
        t.tensors.insert("visual.blocks.0.attn.qkv.bias".into(), TensorEntry { dtype: "F32".into(), shape: Some(vec![30]) });
        let s = scope_of(&json!({}), None, Some(&t), &[]);
        assert_eq!((s.excluded[0].tensors, s.excluded[0].bytes), (2, Some(10 * 20 * 2 + 30 * 4)));
    }
}
