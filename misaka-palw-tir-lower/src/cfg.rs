//! **A `config.json` reader that knows which keys it has read.**
//!
//! Conservative by construction: every getter marks its key as understood, [`Cfg::inert`] marks
//! keys that are known not to change the forward math (dropout, init, token ids, generation
//! defaults …), and [`Cfg::finish`] refuses the model if any key is left over. A key this lowerer
//! has never heard of might change the math, so it is a `NOT_LOWERABLE`, never a silent ignore.
//! The refusal names the keys, so extending the inert list is a reviewed one-line change.

use crate::error::{LowerError, Result};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;

/// Keys every `PretrainedConfig` may carry that cannot change a decoder's inference-time math:
/// bookkeeping, generation defaults (sampling is outside the logits, RFC-0002 non-goals),
/// training-only regularisation and loss terms, and attention-backend selection (every backend
/// computes the same function; the reference is eager).
pub const GLOBAL_INERT: &[&str] = &[
    // bookkeeping
    "_name_or_path",
    "_commit_hash",
    "architectures",
    "model_type",
    "transformers_version",
    "torch_dtype",
    "dtype",
    "use_cache",
    "return_dict",
    "use_return_dict",
    "output_hidden_states",
    "output_attentions",
    "torchscript",
    "use_bfloat16",
    "tf_legacy_loss",
    "pruned_heads",
    "chunk_size_feed_forward",
    "is_decoder",
    "cross_attention_hidden_size",
    "tie_encoder_decoder",
    "finetuning_task",
    "id2label",
    "label2id",
    "num_labels",
    "tokenizer_class",
    "prefix",
    "task_specific_params",
    "problem_type",
    "base_model_tp_plan",
    "base_model_pp_plan",
    "base_model_ep_plan",
    "gradient_checkpointing",
    "_attn_implementation",
    "_attn_implementation_internal",
    "_attn_implementation_autoset",
    "attn_implementation",
    "_experts_implementation",
    // token ids
    "bos_token_id",
    "eos_token_id",
    "pad_token_id",
    "sep_token_id",
    "unk_token_id",
    "decoder_start_token_id",
    // initialisation
    "initializer_range",
    "init_std",
    "initializer_factor",
    // generation defaults
    "max_length",
    "min_length",
    "do_sample",
    "early_stopping",
    "num_beams",
    "num_beam_groups",
    "diversity_penalty",
    "temperature",
    "top_k",
    "top_p",
    "typical_p",
    "repetition_penalty",
    "length_penalty",
    "no_repeat_ngram_size",
    "encoder_no_repeat_ngram_size",
    "bad_words_ids",
    "num_return_sequences",
    "output_scores",
    "return_dict_in_generate",
    "forced_bos_token_id",
    "forced_eos_token_id",
    "remove_invalid_values",
    "exponential_decay_length_penalty",
    "suppress_tokens",
    "begin_suppress_tokens",
    // training-only terms
    "output_router_logits",
    "router_aux_loss_coef",
    "router_z_loss_coef",
    "aux_loss_alpha",
    "seq_aux",
    "attention_dropout",
    "attn_pdrop",
    "resid_pdrop",
    "embd_pdrop",
    "emb_pdrop",
    "hidden_dropout",
    "hidden_dropout_prob",
    "attention_probs_dropout_prob",
    "embedding_dropout",
    "embed_dropout",
    "residual_dropout",
    "resid_dropout",
    "dropout",
    "activation_dropout",
    "classifier_dropout",
    "layerdrop",
    "summary_type",
    "summary_use_proj",
    "summary_activation",
    "summary_proj_to_labels",
    "summary_first_dropout",
];

pub struct Cfg<'a> {
    pub arch: String,
    pub path: String,
    map: &'a Map<String, Value>,
    used: RefCell<BTreeSet<String>>,
}

fn as_usize(v: &Value) -> Option<usize> {
    if let Some(u) = v.as_u64() {
        return usize::try_from(u).ok();
    }
    let f = v.as_f64()?;
    if f >= 0.0 && f.fract() == 0.0 && f < 9.0e15 { Some(f as usize) } else { None }
}

impl<'a> Cfg<'a> {
    pub fn new(arch: impl Into<String>, map: &'a Map<String, Value>, path: impl Into<String>) -> Self {
        let c = Cfg { arch: arch.into(), path: path.into(), map, used: RefCell::new(BTreeSet::new()) };
        c.inert(GLOBAL_INERT);
        c
    }

    fn key(&self, k: &str) -> String {
        if self.path.is_empty() { k.to_string() } else { format!("{}.{}", self.path, k) }
    }

    fn mark(&self, k: &str) {
        self.used.borrow_mut().insert(k.to_string());
    }

    /// Present and not `null`. Does not mark the key as read.
    pub fn has(&self, k: &str) -> bool {
        matches!(self.map.get(k), Some(v) if !v.is_null())
    }

    /// The raw value (marks the key). `null` reads as absent.
    pub fn raw(&self, k: &str) -> Option<&'a Value> {
        self.mark(k);
        match self.map.get(k) {
            Some(Value::Null) | None => None,
            Some(v) => Some(v),
        }
    }

    /// Marks the key. `None`: absent; `Some(None)`: an explicit `null`; `Some(Some(v))`: a value.
    pub fn raw_nullable(&self, k: &str) -> Option<Option<&'a Value>> {
        self.mark(k);
        match self.map.get(k) {
            None => None,
            Some(Value::Null) => Some(None),
            Some(v) => Some(Some(v)),
        }
    }

    pub fn inert(&self, keys: &[&str]) {
        for k in keys {
            self.mark(k);
        }
    }

    pub fn opt_usize(&self, k: &str) -> Result<Option<usize>> {
        match self.raw(k) {
            None => Ok(None),
            Some(v) => {
                as_usize(v).map(Some).ok_or_else(|| LowerError::bad(format!("`{}` is not a non-negative integer: {v}", self.key(k))))
            }
        }
    }
    pub fn req_usize(&self, k: &str) -> Result<usize> {
        self.opt_usize(k)?.ok_or_else(|| LowerError::bad(format!("`{}` is required", self.key(k))))
    }
    pub fn usize_or(&self, k: &str, d: usize) -> Result<usize> {
        Ok(self.opt_usize(k)?.unwrap_or(d))
    }
    /// Absent → the class default `d`; explicit `null` → `None`. (`sliding_window` defaults to 4096
    /// in `MistralConfig` but real checkpoints write `null` to switch it off.)
    pub fn usize_or_null(&self, k: &str, d: Option<usize>) -> Result<Option<usize>> {
        self.mark(k);
        match self.map.get(k) {
            None => Ok(d),
            Some(Value::Null) => Ok(None),
            Some(v) => {
                as_usize(v).map(Some).ok_or_else(|| LowerError::bad(format!("`{}` is not a non-negative integer: {v}", self.key(k))))
            }
        }
    }
    pub fn f64_or_null(&self, k: &str, d: Option<f64>) -> Result<Option<f64>> {
        self.mark(k);
        match self.map.get(k) {
            None => Ok(d),
            Some(Value::Null) => Ok(None),
            Some(v) => v.as_f64().map(Some).ok_or_else(|| LowerError::bad(format!("`{}` is not a number: {v}", self.key(k)))),
        }
    }
    /// The first present key of `keys` (all are marked). For renamed fields (`n_embd`/`hidden_size`).
    pub fn alias_usize(&self, keys: &[&str]) -> Result<Option<usize>> {
        let mut out = None;
        for k in keys {
            let v = self.opt_usize(k)?;
            if out.is_none() {
                out = v;
            } else if let (Some(a), Some(b)) = (out, v)
                && a != b
            {
                return Err(LowerError::bad(format!("aliases {keys:?} disagree: {a} vs {b}")));
            }
        }
        Ok(out)
    }
    pub fn req_alias_usize(&self, keys: &[&str]) -> Result<usize> {
        self.alias_usize(keys)?.ok_or_else(|| LowerError::bad(format!("one of {keys:?} is required")))
    }

    pub fn opt_f64(&self, k: &str) -> Result<Option<f64>> {
        match self.raw(k) {
            None => Ok(None),
            Some(v) => v.as_f64().map(Some).ok_or_else(|| LowerError::bad(format!("`{}` is not a number: {v}", self.key(k)))),
        }
    }
    pub fn f64_or(&self, k: &str, d: f64) -> Result<f64> {
        Ok(self.opt_f64(k)?.unwrap_or(d))
    }
    pub fn req_f64(&self, k: &str) -> Result<f64> {
        self.opt_f64(k)?.ok_or_else(|| LowerError::bad(format!("`{}` is required", self.key(k))))
    }
    pub fn alias_f64(&self, keys: &[&str]) -> Result<Option<f64>> {
        let mut out = None;
        for k in keys {
            let v = self.opt_f64(k)?;
            if out.is_none() {
                out = v;
            }
        }
        Ok(out)
    }

    pub fn opt_bool(&self, k: &str) -> Result<Option<bool>> {
        match self.raw(k) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or_else(|| LowerError::bad(format!("`{}` is not a bool: {v}", self.key(k)))),
        }
    }
    pub fn bool_or(&self, k: &str, d: bool) -> Result<bool> {
        Ok(self.opt_bool(k)?.unwrap_or(d))
    }

    pub fn opt_str(&self, k: &str) -> Result<Option<String>> {
        match self.raw(k) {
            None => Ok(None),
            Some(v) => v
                .as_str()
                .map(|s| Some(s.to_string()))
                .ok_or_else(|| LowerError::bad(format!("`{}` is not a string: {v}", self.key(k)))),
        }
    }
    pub fn str_or(&self, k: &str, d: &str) -> Result<String> {
        Ok(self.opt_str(k)?.unwrap_or_else(|| d.to_string()))
    }

    pub fn opt_list(&self, k: &str) -> Result<Option<&'a Vec<Value>>> {
        match self.raw(k) {
            None => Ok(None),
            Some(v) => v.as_array().map(Some).ok_or_else(|| LowerError::bad(format!("`{}` is not a list", self.key(k)))),
        }
    }
    pub fn opt_usize_list(&self, k: &str) -> Result<Option<Vec<usize>>> {
        match self.opt_list(k)? {
            None => Ok(None),
            Some(l) => l
                .iter()
                .map(|v| as_usize(v).ok_or_else(|| LowerError::bad(format!("`{}` holds a non-integer", self.key(k)))))
                .collect::<Result<Vec<_>>>()
                .map(Some),
        }
    }
    pub fn opt_str_list(&self, k: &str) -> Result<Option<Vec<String>>> {
        match self.opt_list(k)? {
            None => Ok(None),
            Some(l) => l
                .iter()
                .map(|v| {
                    v.as_str().map(str::to_string).ok_or_else(|| LowerError::bad(format!("`{}` holds a non-string", self.key(k))))
                })
                .collect::<Result<Vec<_>>>()
                .map(Some),
        }
    }
    pub fn opt_obj(&self, k: &str) -> Result<Option<&'a Map<String, Value>>> {
        match self.raw(k) {
            None => Ok(None),
            Some(v) => v.as_object().map(Some).ok_or_else(|| LowerError::bad(format!("`{}` is not an object", self.key(k)))),
        }
    }

    /// The key must be absent, `null`, `false`, `0` or an empty list/object: a non-default value
    /// here selects behaviour this lowerer does not implement.
    pub fn forbid(&self, k: &str, why: &str) -> Result<()> {
        match self.raw(k) {
            None => Ok(()),
            Some(Value::Bool(false)) => Ok(()),
            Some(Value::Number(n)) if n.as_f64() == Some(0.0) => Ok(()),
            Some(Value::Array(a)) if a.is_empty() => Ok(()),
            Some(Value::Object(o)) if o.is_empty() => Ok(()),
            Some(v) => Err(LowerError::not_lowerable(format!("{}: `{}` = {v} ({why})", self.arch, self.key(k)))),
        }
    }

    /// The key must be absent/null or equal `want`.
    pub fn require_eq(&self, k: &str, want: &Value, why: &str) -> Result<()> {
        match self.raw(k) {
            None => Ok(()),
            Some(v) if values_equal(v, want) => Ok(()),
            Some(v) => {
                Err(LowerError::not_lowerable(format!("{}: `{}` = {v}, only {want} is implemented ({why})", self.arch, self.key(k))))
            }
        }
    }

    pub fn unknown_keys(&self) -> Vec<String> {
        let used = self.used.borrow();
        self.map.keys().filter(|k| !used.contains(*k)).map(|k| self.key(k)).collect()
    }

    /// Refuse if any key was neither read nor declared inert.
    pub fn finish(&self) -> Result<()> {
        let unknown = self.unknown_keys();
        if unknown.is_empty() {
            Ok(())
        } else {
            Err(LowerError::not_lowerable(format!(
                "{}: config key(s) this lowerer does not model: {} — a key that might change the math is refused, not ignored",
                self.arch,
                unknown.join(", ")
            )))
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_keys_are_refused_and_named() {
        let v = json!({"hidden_size": 8, "mystery_knob": 3, "bos_token_id": 1, "rms_norm_eps": null});
        let c = Cfg::new("LlamaForCausalLM", v.as_object().unwrap(), "");
        assert_eq!(c.req_usize("hidden_size").unwrap(), 8);
        let e = c.finish().unwrap_err();
        match e {
            LowerError::NotLowerable(s) => {
                assert!(s.contains("mystery_knob"), "{s}");
                assert!(!s.contains("bos_token_id"));
                assert!(s.contains("rms_norm_eps"), "a key never read is unknown even when null: {s}");
            }
            other => panic!("{other:?}"),
        }
        c.inert(&["mystery_knob", "rms_norm_eps"]);
        c.finish().unwrap();
    }

    #[test]
    fn forbid_accepts_defaults_and_refuses_the_rest() {
        let v = json!({"a": false, "b": 0, "c": [], "d": true, "e": 2.5, "f": null});
        let c = Cfg::new("X", v.as_object().unwrap(), "");
        for k in ["a", "b", "c", "f", "absent"] {
            c.forbid(k, "test").unwrap();
        }
        assert!(c.forbid("d", "test").is_err());
        assert!(c.forbid("e", "test").is_err());
    }

    #[test]
    fn integral_floats_read_as_integers_and_aliases_must_agree() {
        let v = json!({"n_embd": 64.0, "hidden_size": 64, "x": 1.5});
        let c = Cfg::new("X", v.as_object().unwrap(), "");
        assert_eq!(c.alias_usize(&["hidden_size", "n_embd"]).unwrap(), Some(64));
        assert!(c.opt_usize("x").is_err());
        let v = json!({"n_embd": 32, "hidden_size": 64});
        let c = Cfg::new("X", v.as_object().unwrap(), "");
        assert!(c.alias_usize(&["hidden_size", "n_embd"]).is_err());
    }
}
