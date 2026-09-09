//! **ADR-0096 Decision 1 — the OpenAI surface, spelled once, and everything it refuses by name.**
//!
//! A client written against `api.openai.com` sends four ordinary things this lane used to answer
//! with a 400: `content` as a list of parts, a `tool` turn or a `tools` list, a `response_format`,
//! and the sampling knobs its SDK sets by default. This module is the one place the request shape
//! is read, and [`admit_request`] is the one function every refusal comes from — called BEFORE the
//! worker is touched, so a refusal costs a 4xx and never an inference.
//!
//! **The doctrine, in code.** A request the lane cannot honour is refused BY NAME before the
//! inference, and nothing is downgraded silently (ADR-0082 Decision 11's rationale, kept as a rule
//! of ADR-0096). Two deliberate, REPORTED exceptions: a sampling knob at its identity value
//! (`top_p: 1`, `top_k: 0`, `min_p: 0`, `repeat_penalty: 1`, `frequency_penalty: 0`,
//! `presence_penalty: 0`), which a stock SDK sends by default and which asks for nothing, is
//! accepted and listed in `misaka.sampling.requested`; and the fields OpenAI defines as having no
//! effect on the answer (`user`, `metadata`, `store`, `parallel_tool_calls`) are accepted and
//! listed in `misaka.ignored_fields`. A field this module does not name is refused, never dropped —
//! a field that is silently dropped is a request that was silently changed.
//!
//! **What is NOT here.** `sampling_from_request` (ADR-0082 Decision 11's fence, in `main.rs`) is
//! called, not copied: the temperature and seed rule is the chain's and has one spelling. The
//! template and the tool convention are `wire`'s. The schema subset is `misaka-palw-constraint`'s.

use std::collections::BTreeMap;

use kaspa_consensus_core::palw_freeprompt_v3::PalwFpWorkerManifestV1;
use kaspa_hashes::Hash64;
use misaka_palw_constraint::schema::Schema;
use serde::Deserialize;
use serde_json::Value;

use crate::chain::ChainFacts;
use crate::wire::{ChatTurn, ToolCallTurn, ToolChoice, ToolSpec};

// ---------------------------------------------------------------------------------------------
// The request, as OpenAI spells it
// ---------------------------------------------------------------------------------------------

/// OpenAI's `messages[].tool_calls[]`.
#[derive(Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct ToolCallSpec {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    pub function: ToolCallFunctionSpec,
    /// Some SDKs replay the streaming `index`; it says nothing about the call.
    #[serde(default)]
    pub index: Option<u64>,
}

/// OpenAI's `tool_calls[].function`: a name and its arguments, which OpenAI carries as a JSON
/// STRING and some clients carry as the object itself. Both are read; neither is repaired.
#[derive(Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct ToolCallFunctionSpec {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// One message. `content` is read as a `Value` because OpenAI allows a string, a list of parts, or
/// `null` (an assistant turn that only made calls), and the refusal for a non-text part has to
/// name the part's type and position — which a typed enum would swallow into "invalid type".
#[derive(Deserialize, Default, Clone)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallSpec>>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    /// The fields OpenAI's own response message carries and a client replays verbatim
    /// (`refusal: null`, `annotations: []`, `audio: null`, `function_call: null`). Read here so a
    /// non-null `audio` or `function_call` can be refused by name and the null ones reported.
    #[serde(default)]
    pub refusal: Option<Value>,
    #[serde(default)]
    pub annotations: Option<Value>,
    #[serde(default)]
    pub audio: Option<Value>,
    #[serde(default)]
    pub function_call: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// OpenAI's `tools[]`.
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionDefinition,
}

/// OpenAI's `tools[].function`.
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Option<Value>,
    #[serde(default)]
    pub strict: Option<bool>,
}

/// The `misaka` request extension (ADR-0096 Decision 3).
#[derive(Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct MisakaRequestExt {
    /// The caller needs the shape of the answer to be COMMITTED — replayed by the seat, triable by
    /// the court — and must never receive an advisory lookalike. Refused by name while
    /// `ChainFacts::fp_decode_constraint_armed` is false.
    #[serde(default)]
    pub require_committed_format: bool,
}

/// OpenAI's `seed` is an integer; this lane's (ADR-0082 Decision 11) is 64 hex characters. An
/// integer is read as its decimal text so that it reaches `sampling_from_request`'s own rule and
/// is refused THERE, by that rule's sentence, rather than by a serde type error that names no
/// field.
fn seed_as_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(other) => Err(serde::de::Error::custom(format!("seed must be 64 hex characters, got {}", kind_of(&other)))),
    }
}

/// The chat completion request — every field OpenAI's surface names, and this lane's own.
#[derive(Deserialize, Default)]
pub struct ChatRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// OpenAI's newer spelling of `max_tokens`; the two are one field here.
    #[serde(default)]
    pub max_completion_tokens: Option<u32>,
    /// ADR-0077 Decision 2: the answer streams as SSE; the commitment does not.
    #[serde(default)]
    pub stream: Option<bool>,
    /// Only `{"include_usage": bool}`; any other key is refused by name.
    #[serde(default)]
    pub stream_options: Option<serde_json::Map<String, Value>>,
    /// ADR-0078: the kind the person asked for — a transformer name (`scene/glb/v1`) or a kind
    /// name (`scene`). Absent: the answer is the product and nothing is derived.
    #[serde(default)]
    pub derive: Option<String>,
    /// ADR-0078 Decision 6: elect this claim's DSL into the data-availability obligation, so
    /// third parties can verify the derivation on request. Default off — the DSL is the answer
    /// to the person's prompt, and it is theirs to publish.
    #[serde(default)]
    pub serve_dsl: bool,
    /// **ADR-0082 Decision 11: the sampling temperature the person asked for**, as an ordinary
    /// float, in the class's own logit units — the number an OpenAI-shaped client already sends.
    /// Quantized to Q24 by `sampling_from_request` and carried into the job id; absent or `0.0`
    /// is the greedy default, which is the only value a network without the fence admits.
    #[serde(default)]
    pub temperature: Option<f64>,
    /// ADR-0082 Decision 11: 64 hex characters, the seed the sampler draws under. Absent is the
    /// zero seed. Named by the requester rather than rolled here so the same request twice is the
    /// same answer twice — and so nothing about the draw is the gateway's to choose.
    #[serde(default, deserialize_with = "seed_as_text")]
    pub seed: Option<String>,
    // ADR-0096 Decision 4: knobs with no consensus rule, accepted at their identity value only.
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub top_k: Option<f64>,
    #[serde(default)]
    pub min_p: Option<f64>,
    #[serde(default)]
    pub repeat_penalty: Option<f64>,
    #[serde(default)]
    pub frequency_penalty: Option<f64>,
    #[serde(default)]
    pub presence_penalty: Option<f64>,
    #[serde(default)]
    pub logit_bias: Option<Value>,
    #[serde(default)]
    pub stop: Option<Value>,
    // Refused by name unless they ask for nothing.
    #[serde(default)]
    pub n: Option<u32>,
    #[serde(default)]
    pub logprobs: Option<Value>,
    #[serde(default)]
    pub top_logprobs: Option<Value>,
    #[serde(default)]
    pub functions: Option<Value>,
    #[serde(default)]
    pub function_call: Option<Value>,
    // ADR-0096 Decision 2.
    #[serde(default)]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(default)]
    pub parallel_tool_calls: Option<bool>,
    // ADR-0096 Decision 3.
    #[serde(default)]
    pub response_format: Option<Value>,
    // OpenAI's no-effect fields: accepted, listed in `misaka.ignored_fields`.
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub store: Option<bool>,
    #[serde(default)]
    pub misaka: Option<MisakaRequestExt>,
    /// Everything else, refused by name in [`admit_request`].
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

// ---------------------------------------------------------------------------------------------
// What was admitted
// ---------------------------------------------------------------------------------------------

/// `response_format`, as admitted (ADR-0096 Decision 3).
#[derive(Clone, Debug)]
pub enum FormatKind {
    /// Any single JSON value.
    JsonObject,
    /// A JSON value conforming to the parsed schema.
    JsonSchema,
}

/// A `response_format` that asked for a shape: what the model is told, what the answer is checked
/// against, and the bytes the constraint is named by.
#[derive(Clone, Debug)]
pub struct FormatRequest {
    pub kind: FormatKind,
    /// The `json_schema.name`, reported back; it is not rendered into the prompt.
    pub name: Option<String>,
    pub schema: Option<Schema>,
    /// RFC 8785 bytes of the schema as sent — the text the model sees and the id's preimage.
    pub canonical_schema: Option<Vec<u8>>,
    /// `constraint_id(canonical_schema)`. Advisory today: it names the constraint the request
    /// asked for, and nothing on chain carries it until Part B.
    pub constraint_id: Option<Hash64>,
}

/// What a `response_format` check found, after the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatReport {
    pub valid: bool,
    pub errors: Vec<String>,
    /// SHA-256 of the answer's RFC 8785 bytes when it is valid — what a consumer that stored the
    /// canonical JSON can check its copy against.
    pub canonical_sha256: Option<String>,
}

impl FormatRequest {
    /// The kind's name in `misaka.format.requested.type`.
    pub fn type_name(&self) -> &'static str {
        match self.kind {
            FormatKind::JsonObject => "json_object",
            FormatKind::JsonSchema => "json_schema",
        }
    }

    /// The sentence appended to the system turn (ADR-0096 Decision 3, advisory mode): the person
    /// asked for a shape, so the model is asked for it in text — F1 is intact, and the run is
    /// unconstrained.
    pub fn instruction(&self) -> String {
        match self.kind {
            FormatKind::JsonObject => "\n\nRespond with a single JSON value and nothing else.".to_string(),
            FormatKind::JsonSchema => format!(
                "\n\nRespond with a single JSON value that conforms to this JSON Schema and nothing else:\n{}",
                String::from_utf8_lossy(self.canonical_schema.as_deref().unwrap_or_default())
            ),
        }
    }

    /// **Check the shown answer** — whitespace trimmed, a code fence refused rather than unwrapped
    /// (unwrapping would be the entrance changing the answer to make it parse, which ADR-0078
    /// Decision 2 forbids), parsed as ONE JSON value, validated against the schema when there is
    /// one. Nothing committed moves: this reads the display string.
    pub fn check(&self, shown: &str) -> FormatReport {
        let trimmed = shown.trim();
        if trimmed.starts_with("```") {
            return FormatReport {
                valid: false,
                errors: vec!["the answer is wrapped in a code fence".to_string()],
                canonical_sha256: None,
            };
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(e) => {
                return FormatReport {
                    valid: false,
                    errors: vec![format!("the answer is not a single JSON value: {e}")],
                    canonical_sha256: None,
                };
            }
        };
        let errors = match &self.schema {
            Some(schema) => misaka_palw_constraint::schema::validate(schema, &value).err().unwrap_or_default(),
            None => Vec::new(),
        };
        if !errors.is_empty() {
            return FormatReport { valid: false, errors, canonical_sha256: None };
        }
        match misaka_palw_constraint::canonical::to_rfc8785(&value) {
            Ok(bytes) => {
                use sha2::Digest as _;
                let digest = sha2::Sha256::digest(&bytes);
                FormatReport { valid: true, errors: Vec::new(), canonical_sha256: Some(faster_hex::hex_string(&digest)) }
            }
            Err(why) => {
                FormatReport { valid: false, errors: vec![format!("the answer has no canonical form: {why}")], canonical_sha256: None }
            }
        }
    }

    /// `misaka.format` (ADR-0096 Decision 3): what was asked, which mode served it, and what the
    /// check found. `enforcement` is `"advisory"` on every network whose fence is dormant — which
    /// is every network this build can reach (`ChainFacts::fp_decode_constraint_armed`).
    pub fn report_json(&self, report: &FormatReport) -> Value {
        serde_json::json!({
            "requested": {
                "type": self.type_name(),
                "name": self.name,
                "constraint_id": self.constraint_id.map(|id| faster_hex::hex_string(id.as_byte_slice())),
                "constraint_bytes": self.canonical_schema.as_ref().map(Vec::len),
            },
            "enforcement": "advisory",
            "valid": report.valid,
            "errors": report.errors,
            "canonical_sha256": report.canonical_sha256,
        })
    }
}

/// **What the entrance admitted** — the request reduced to what the template, the worker and the
/// response builder need, with every refusal already behind it.
#[derive(Debug)]
pub struct AdmittedRequest {
    pub turns: Vec<ChatTurn>,
    pub tools: Vec<ToolSpec>,
    pub tool_choice: ToolChoice,
    /// Whether the request said anything about `tool_choice` at all (for the report).
    pub tool_choice_given: bool,
    pub format: Option<FormatRequest>,
    /// `max_tokens`, or its alias. The gateway clamps it to its cap.
    pub max_tokens: Option<u32>,
    /// ADR-0082 Decision 11's `(sampling_seed, temperature_q)`, gated on the chain.
    pub sampling: ([u8; 32], u32),
    /// The sampling knobs as the client sent them, for `misaka.sampling.requested`.
    pub sampling_requested: serde_json::Map<String, Value>,
    /// The identity-valued knobs that were sent — reported as `not_a_rule_on_this_lane`.
    pub not_a_rule_on_this_lane: Vec<&'static str>,
    /// Accepted fields that changed nothing, by name.
    pub ignored_fields: Vec<String>,
    pub include_usage: bool,
}

/// The identity value of each knob ADR-0096 Decision 4 names: the value a stock SDK sends by
/// default, which asks for nothing.
const IDENTITY_KNOBS: [(&str, f64); 6] =
    [("top_p", 1.0), ("top_k", 0.0), ("min_p", 0.0), ("repeat_penalty", 1.0), ("frequency_penalty", 0.0), ("presence_penalty", 0.0)];

/// OpenAI's own rule for a function name, kept because the name is rendered into the prompt
/// unquoted inside `"name": "…"` and a name outside this set would be text the template did not
/// write.
fn is_function_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn not_a_rule(name: &str, value: impl std::fmt::Display) -> String {
    format!("{name} {value} is not a rule on this lane (ADR-0096 Decision 4): the seat replays a greedy decode and nothing else")
}

/// **Parse a request body and admit it** — the route's and the conformance corpus's ONE path.
pub fn parse_and_admit(body: &[u8], facts: &ChainFacts) -> Result<(ChatRequest, AdmittedRequest), String> {
    let chat: ChatRequest = serde_json::from_slice(body).map_err(|e| format!("request body is not a chat completion: {e}"))?;
    let admitted = admit_request(&chat, facts)?;
    Ok((chat, admitted))
}

/// **Every refusal, in one function, before the worker** (ADR-0096 Decision 1; invariant 6).
///
/// The order is the order a person can act on: fields this surface does not serve; the legacy
/// and multi-choice fields; the sampler (ADR-0082's fence first, then Decision 4's knobs); the
/// messages; the tools; the format; the extension. Every arm names the field, the value where it
/// helps, and the rule.
pub fn admit_request(chat: &ChatRequest, facts: &ChainFacts) -> Result<AdmittedRequest, String> {
    let mut ignored: Vec<String> = Vec::new();

    // Fields this surface does not name are refused, never dropped.
    if !chat.extra.is_empty() {
        let names: Vec<String> = chat.extra.keys().map(|k| format!("`{k}`")).collect();
        return Err(format!(
            "{} {} not a field this surface serves (ADR-0096 Decision 1 names every accepted field) and {} refused rather than \
             dropped: a field that is silently dropped is a request that was silently changed",
            names.join(", "),
            if names.len() == 1 { "is" } else { "are" },
            if names.len() == 1 { "is" } else { "are" },
        ));
    }
    if chat.functions.as_ref().is_some_and(|v| !v.is_null()) {
        return Err(
            "`functions` is OpenAI's legacy tool surface and is refused by name (ADR-0096 Decision 1); send `tools`".to_string()
        );
    }
    if chat.function_call.as_ref().is_some_and(|v| !v.is_null()) {
        return Err(
            "`function_call` is OpenAI's legacy tool surface and is refused by name (ADR-0096 Decision 1); send `tool_choice`"
                .to_string(),
        );
    }
    if let Some(n) = chat.n
        && n != 1
    {
        return Err(format!(
            "n {n} is refused by name (ADR-0096 Decision 1): one inference is one claim (ADR-0077 R0), so this surface returns exactly \
             one choice"
        ));
    }
    if chat.logprobs.as_ref().is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false))) {
        return Err(
            "logprobs is refused by name (ADR-0096 Decision 1): the lane commits token ids, not logits, and a number the court \
                    cannot try is not returned"
                .to_string(),
        );
    }
    if chat.top_logprobs.as_ref().is_some_and(|v| !v.is_null()) {
        return Err("top_logprobs is refused by name (ADR-0096 Decision 1): the lane commits token ids, not logits".to_string());
    }
    let max_tokens = match (chat.max_tokens, chat.max_completion_tokens) {
        (Some(a), Some(b)) if a != b => {
            return Err(format!(
                "max_tokens {a} and max_completion_tokens {b} disagree; the two are one field on this surface (ADR-0096 Decision 1) — \
                 send either"
            ));
        }
        (a, b) => a.or(b),
    };
    let mut include_usage = false;
    if let Some(options) = &chat.stream_options {
        for (key, value) in options {
            match key.as_str() {
                "include_usage" => {
                    include_usage = value
                        .as_bool()
                        .ok_or_else(|| format!("stream_options.include_usage is {} where a boolean was expected", kind_of(value)))?;
                }
                other => {
                    return Err(format!(
                        "stream_options.{other} is refused by name (ADR-0096 Decision 1): only include_usage is served on this surface"
                    ));
                }
            }
        }
    }

    // ADR-0082 Decision 11 first — its refusal names the fence — then Decision 4's knobs.
    let sampling = crate::sampling_from_request(chat, facts)?;
    let mut sampling_requested = serde_json::Map::new();
    if let Some(t) = chat.temperature {
        sampling_requested.insert("temperature".into(), serde_json::json!(t));
    }
    if let Some(seed) = &chat.seed {
        sampling_requested.insert("seed".into(), serde_json::json!(seed));
    }
    let mut not_a_rule_on_this_lane = Vec::new();
    let knobs = [chat.top_p, chat.top_k, chat.min_p, chat.repeat_penalty, chat.frequency_penalty, chat.presence_penalty];
    for ((name, identity), sent) in IDENTITY_KNOBS.iter().zip(knobs) {
        if let Some(value) = sent {
            if value != *identity {
                return Err(not_a_rule(name, value));
            }
            sampling_requested.insert((*name).to_string(), serde_json::json!(value));
            not_a_rule_on_this_lane.push(*name);
        }
    }
    if let Some(bias) = &chat.logit_bias
        && !matches!(bias, Value::Null)
        && bias.as_object().is_none_or(|m| !m.is_empty())
    {
        return Err(not_a_rule("logit_bias", bias));
    }
    if let Some(stop) = &chat.stop {
        let empty = match stop {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Array(items) => items.is_empty(),
            _ => false,
        };
        if !empty {
            return Err(format!(
                "stop {stop} is not a rule on this lane (ADR-0096 Decision 4): execution runs to the declared budget and the display cut \
                 is the template's — trim the answer in your app"
            ));
        }
    }

    // The messages.
    if chat.messages.len() > crate::MAX_CHAT_MESSAGES {
        return Err(format!("{} messages exceeds the {}-message cap", chat.messages.len(), crate::MAX_CHAT_MESSAGES));
    }
    let mut turns: Vec<ChatTurn> = Vec::with_capacity(chat.messages.len());
    for (i, message) in chat.messages.iter().enumerate() {
        if !matches!(message.role.as_str(), "system" | "user" | "assistant" | "tool") {
            return Err(format!(
                "messages[{i}].role {:?} is not one of system|user|assistant|tool (ADR-0096 Decision 2 adds `tool`)",
                message.role
            ));
        }
        if !message.extra.is_empty() {
            let names: Vec<String> = message.extra.keys().map(|k| format!("`{k}`")).collect();
            return Err(format!(
                "messages[{i}] carries {} which this surface does not serve (ADR-0096 Decision 1); refused rather than dropped",
                names.join(", ")
            ));
        }
        if message.audio.as_ref().is_some_and(|v| !v.is_null()) {
            return Err(format!("messages[{i}].audio is refused by name (ADR-0096 Decision 1): this lane carries text only"));
        }
        if message.function_call.as_ref().is_some_and(|v| !v.is_null()) {
            return Err(format!(
                "messages[{i}].function_call is OpenAI's legacy tool surface and is refused by name (ADR-0096 Decision 1); send tool_calls"
            ));
        }
        for (present, name) in [
            (message.name.is_some(), "messages[].name"),
            (message.tool_call_id.is_some(), "messages[].tool_call_id"),
            (message.refusal.is_some(), "messages[].refusal"),
            (message.annotations.is_some(), "messages[].annotations"),
            (message.audio.is_some(), "messages[].audio"),
            (message.function_call.is_some(), "messages[].function_call"),
            (message.tool_calls.as_ref().is_some_and(|calls| calls.iter().any(|c| c.id.is_some())), "messages[].tool_calls[].id"),
            (
                message.tool_calls.as_ref().is_some_and(|calls| calls.iter().any(|c| c.index.is_some())),
                "messages[].tool_calls[].index",
            ),
        ] {
            if present && !ignored.iter().any(|n| n == name) {
                ignored.push(name.to_string());
            }
        }
        let content = flatten_content(i, message.content.as_ref())?;
        let mut tool_calls = Vec::new();
        if let Some(calls) = &message.tool_calls
            && !calls.is_empty()
        {
            if message.role != "assistant" {
                return Err(format!(
                    "messages[{i}].tool_calls on a {} message: only an assistant turn makes calls (ADR-0096 Decision 2)",
                    message.role
                ));
            }
            for (j, call) in calls.iter().enumerate() {
                if let Some(kind) = &call.kind
                    && kind != "function"
                {
                    return Err(format!(
                        "messages[{i}].tool_calls[{j}].type {kind:?} is refused by name: only `function` calls exist on this surface"
                    ));
                }
                if !is_function_name(&call.function.name) {
                    return Err(format!(
                        "messages[{i}].tool_calls[{j}].function.name {:?} is not a function name ([A-Za-z0-9_-]{{1,64}})",
                        call.function.name
                    ));
                }
                let arguments = match &call.function.arguments {
                    None | Some(Value::Null) => serde_json::json!({}),
                    Some(Value::String(text)) => serde_json::from_str::<Value>(text).map_err(|e| {
                        format!(
                            "messages[{i}].tool_calls[{j}].function.arguments is not JSON ({e}): the template renders a call's arguments \
                             as a JSON value and this lane does not repair them (ADR-0096 Decision 2)"
                        )
                    })?,
                    Some(object @ Value::Object(_)) => object.clone(),
                    Some(other) => {
                        return Err(format!(
                            "messages[{i}].tool_calls[{j}].function.arguments is {} where a JSON string or object was expected",
                            kind_of(other)
                        ));
                    }
                };
                tool_calls.push(ToolCallTurn { name: call.function.name.clone(), arguments });
            }
        }
        let mut turn = ChatTurn::text(&message.role, &content);
        turn.tool_calls = tool_calls;
        turns.push(turn);
    }
    if !turns.iter().any(|t| t.role == "user" || t.role == "tool") {
        return Err("the request carries no user message".to_string());
    }

    // The tools (ADR-0096 Decision 2).
    let mut tools: Vec<ToolSpec> = Vec::new();
    for (i, tool) in chat.tools.iter().flatten().enumerate() {
        if tool.kind != "function" {
            return Err(format!("tools[{i}].type {:?} is refused by name: only `function` tools exist on this surface", tool.kind));
        }
        let function = &tool.function;
        if !is_function_name(&function.name) {
            return Err(format!(
                "tools[{i}].function.name {:?} is not a function name ([A-Za-z0-9_-]{{1,64}}); a name outside that set would ride into \
                 the prompt as text the template did not write",
                function.name
            ));
        }
        if tools.iter().any(|t| t.name == function.name) {
            return Err(format!("tools[{i}].function.name {:?} is declared twice", function.name));
        }
        if let Some(parameters) = &function.parameters
            && !parameters.is_object()
        {
            return Err(format!("tools[{i}].function.parameters is {} where a JSON Schema object was expected", kind_of(parameters)));
        }
        if function.strict == Some(true) {
            // Under Part B a strict call compiles to a decode constraint (Decision 2), so a strict
            // schema outside the subset is refused now, in the advisory mode too — the same rule
            // `response_format` follows, for the same reason.
            let parameters = function.parameters.clone().unwrap_or_else(|| serde_json::json!({}));
            misaka_palw_constraint::schema::parse(&parameters).map_err(|e| {
                format!(
                    "tools[{i}].function.parameters with strict: true must be in the schema subset (ADR-0096 Decision 2: under Part B a \
                     strict call compiles to a decode constraint): {e}"
                )
            })?;
        }
        tools.push(ToolSpec {
            name: function.name.clone(),
            description: function.description.clone(),
            parameters: function.parameters.clone(),
            strict: function.strict,
        });
    }
    let tool_choice = match &chat.tool_choice {
        None | Some(Value::Null) => ToolChoice::Auto,
        Some(Value::String(s)) => match s.as_str() {
            "auto" => ToolChoice::Auto,
            "none" => ToolChoice::None,
            "required" => ToolChoice::Required,
            other => {
                return Err(format!(
                    "tool_choice {other:?} is refused by name: auto | none | required | {{type: \"function\", function: {{name}}}}"
                ));
            }
        },
        Some(object @ Value::Object(_)) => {
            let name = object.get("function").and_then(|f| f.get("name")).and_then(Value::as_str);
            match (object.get("type").and_then(Value::as_str), name) {
                (Some("function"), Some(name)) => ToolChoice::Named(name.to_string()),
                _ => {
                    return Err(format!(
                        "tool_choice {object} is refused by name: an object choice is {{type: \"function\", function: {{name}}}}"
                    ));
                }
            }
        }
        Some(other) => return Err(format!("tool_choice is {} where a string or an object was expected", kind_of(other))),
    };
    match &tool_choice {
        ToolChoice::Required if tools.is_empty() => {
            return Err("tool_choice \"required\" asks for a function call and the request declares no tools".to_string());
        }
        ToolChoice::Named(name) if !tools.iter().any(|t| &t.name == name) => {
            return Err(format!("tool_choice names `{name}`, which `tools` does not declare"));
        }
        _ => {}
    }
    if chat.parallel_tool_calls.is_some() {
        ignored.push("parallel_tool_calls".to_string());
    }

    // The format (ADR-0096 Decision 3).
    let format = admit_response_format(chat.response_format.as_ref())?;
    if chat.misaka.as_ref().is_some_and(|m| m.require_committed_format) && !facts.fp_decode_constraint_armed {
        return Err(
            "committed format needs Params::palw_fp_decode_constraint (ADR-0096 Decision 8), which this network has not armed; \
                    ask for enforcement \"advisory\" or wait for the fence"
                .to_string(),
        );
    }

    // OpenAI's no-effect fields.
    for (present, name) in [(chat.user.is_some(), "user"), (chat.metadata.is_some(), "metadata"), (chat.store.is_some(), "store")] {
        if present {
            ignored.push(name.to_string());
        }
    }

    Ok(AdmittedRequest {
        turns,
        tools,
        tool_choice,
        tool_choice_given: chat.tool_choice.as_ref().is_some_and(|v| !v.is_null()),
        format,
        max_tokens,
        sampling,
        sampling_requested,
        not_a_rule_on_this_lane,
        ignored_fields: ignored,
        include_usage,
    })
}

/// `messages[i].content`: a string, a list of `{type: "text"}` parts flattened with `\n`, or
/// `null`. A non-text part is refused by NAME with its position — the class's model reads token
/// ids and nothing else, and an image the entrance dropped would be a prompt the person did not
/// write.
fn flatten_content(i: usize, content: Option<&Value>) -> Result<String, String> {
    match content {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(parts)) => {
            let mut texts: Vec<&str> = Vec::with_capacity(parts.len());
            for (j, part) in parts.iter().enumerate() {
                let Some(members) = part.as_object() else {
                    return Err(format!("messages[{i}].content[{j}] is {} where a {{type, …}} part was expected", kind_of(part)));
                };
                match members.get("type").and_then(Value::as_str) {
                    Some("text") => match members.get("text").and_then(Value::as_str) {
                        Some(text) => texts.push(text),
                        None => return Err(format!("messages[{i}].content[{j}] is a text part without a `text` string")),
                    },
                    Some(other) => {
                        return Err(format!(
                            "messages[{i}].content[{j}] is a `{other}` part, and this lane carries text only (ADR-0096 Decision 1): the \
                             class's model reads token ids and nothing else, so the part is refused rather than dropped"
                        ));
                    }
                    None => return Err(format!("messages[{i}].content[{j}] is a part with no `type`")),
                }
            }
            Ok(texts.join("\n"))
        }
        Some(other) => Err(format!("messages[{i}].content is {} where a string or a list of parts was expected", kind_of(other))),
    }
}

/// `response_format` (ADR-0096 Decision 3): `text` asks nothing; `json_object` and `json_schema`
/// become a [`FormatRequest`]; a schema outside the subset is refused by name in both modes.
fn admit_response_format(format: Option<&Value>) -> Result<Option<FormatRequest>, String> {
    let Some(format) = format else { return Ok(None) };
    if format.is_null() {
        return Ok(None);
    }
    let members = format.as_object().ok_or_else(|| format!("response_format is {} where an object was expected", kind_of(format)))?;
    match members.get("type").and_then(Value::as_str) {
        Some("text") => Ok(None),
        Some("json_object") => Ok(Some(FormatRequest {
            kind: FormatKind::JsonObject,
            name: None,
            schema: None,
            canonical_schema: None,
            constraint_id: None,
        })),
        Some("json_schema") => {
            let spec = members.get("json_schema").and_then(Value::as_object).ok_or_else(|| {
                "response_format.json_schema is missing (an object with `schema` and optionally `name`, `strict`)".to_string()
            })?;
            for key in spec.keys() {
                if !matches!(key.as_str(), "name" | "schema" | "strict" | "description") {
                    return Err(format!("response_format.json_schema.{key} is refused by name: name, schema, strict, description"));
                }
            }
            let schema_value = spec.get("schema").ok_or_else(|| "response_format.json_schema.schema is missing".to_string())?;
            let schema =
                misaka_palw_constraint::schema::parse(schema_value).map_err(|e| format!("response_format.json_schema.schema: {e}"))?;
            let canonical = misaka_palw_constraint::canonical::to_rfc8785(schema_value)
                .map_err(|e| format!("response_format.json_schema.schema has no canonical form: {e}"))?;
            if canonical.len() > misaka_palw_constraint::PALW_CONSTRAINT_MAX_BYTES {
                return Err(format!(
                    "response_format.json_schema.schema canonicalizes to {} bytes and the constraint cap is {} (ADR-0096 Decision 7)",
                    canonical.len(),
                    misaka_palw_constraint::PALW_CONSTRAINT_MAX_BYTES
                ));
            }
            let constraint_id = misaka_palw_constraint::constraint_id(&canonical);
            Ok(Some(FormatRequest {
                kind: FormatKind::JsonSchema,
                name: spec.get("name").and_then(Value::as_str).map(str::to_string),
                schema: Some(schema),
                canonical_schema: Some(canonical),
                constraint_id: Some(constraint_id),
            }))
        }
        Some(other) => Err(format!("response_format.type {other:?} is refused by name: text | json_object | json_schema")),
        None => Err("response_format has no `type`".to_string()),
    }
}

// ---------------------------------------------------------------------------------------------
// GET /v1/models
// ---------------------------------------------------------------------------------------------

/// The one model id this surface answers to. `model` in a request is echoed, never matched — a
/// gateway serves exactly one class, and the class is what `/health` and this list name.
pub const MODEL_ID: &str = "misaka-palw-fp-v3";

/// `GET /v1/models` (ADR-0096 Decision 1): the one class this gateway serves, as `misaka-palw-fp-v3`
/// plus the class's own names — the id a chain registers, the model the manifest names, the width
/// and the template. `created` is the gateway's boot time: the moment this model became reachable
/// here, and a field OpenAI's SDKs require.
pub fn models_body(class_id_hex: &str, manifest: &PalwFpWorkerManifestV1, template_id: &str, created: u64) -> Value {
    serde_json::json!({
        "object": "list",
        "data": [{
            "id": MODEL_ID,
            "object": "model",
            "created": created,
            "owned_by": "misaka-palw-gateway",
            "misaka": {
                "class_id": class_id_hex,
                "model_id": manifest.model_id,
                "n_ctx": manifest.n_ctx,
                "template_id": template_id,
                "runtime_manifest_hash": faster_hex::hex_string(manifest.runtime_manifest_hash.as_byte_slice()),
            },
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dormant() -> ChainFacts {
        ChainFacts::default()
    }

    fn admit(request: Value) -> Result<AdmittedRequest, String> {
        parse_and_admit(&serde_json::to_vec(&request).unwrap(), &dormant()).map(|(_, admitted)| admitted)
    }

    fn user(text: &str) -> Value {
        json!([{ "role": "user", "content": text }])
    }

    fn weather_tool() -> Value {
        json!({ "type": "function", "function": { "name": "get_weather", "parameters": { "type": "object", "properties": { "city": { "type": "string" } }, "required": ["city"] } } })
    }

    /// **ADR-0096 Decision 4 at the entrance.** A knob at its identity value is accepted, listed
    /// in `sampling.requested` and named `not_a_rule_on_this_lane`; at any other value it is
    /// refused by name with the value; the temperature and seed rule is ADR-0082's, reached through
    /// the one spelling of it — and an integer seed lands on that rule's own sentence.
    #[test]
    fn identity_valued_knobs_are_accepted_and_reported_and_any_other_value_is_refused_by_name() {
        let all_identity = json!({
            "messages": user("hi"), "temperature": 0, "top_p": 1, "top_k": 0, "min_p": 0,
            "repeat_penalty": 1, "frequency_penalty": 0, "presence_penalty": 0, "logit_bias": {}, "stop": []
        });
        let admitted = admit(all_identity).expect("identity values ask for nothing");
        assert_eq!(
            admitted.not_a_rule_on_this_lane,
            vec!["top_p", "top_k", "min_p", "repeat_penalty", "frequency_penalty", "presence_penalty"]
        );
        assert_eq!(admitted.sampling_requested.len(), 7, "temperature and the six knobs, as sent: {:?}", admitted.sampling_requested);
        assert_eq!(admitted.sampling_requested["top_p"], json!(1.0));
        assert_eq!(admitted.sampling, (kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY, 0));

        for (name, value) in [
            ("top_p", 0.9),
            ("top_k", 40.0),
            ("min_p", 0.05),
            ("repeat_penalty", 1.1),
            ("frequency_penalty", 0.5),
            ("presence_penalty", -0.2),
        ] {
            let err = admit(json!({ "messages": user("hi"), name: value })).unwrap_err();
            assert_eq!(
                err,
                format!(
                    "{name} {value} is not a rule on this lane (ADR-0096 Decision 4): the seat replays a greedy decode and nothing else"
                )
            );
        }
        let err = admit(json!({ "messages": user("hi"), "logit_bias": { "50256": -100 } })).unwrap_err();
        assert!(err.starts_with("logit_bias {\"50256\":-100} is not a rule on this lane"), "{err}");
        let err = admit(json!({ "messages": user("hi"), "stop": ["\n"] })).unwrap_err();
        assert!(err.contains("stop") && err.contains("not a rule on this lane (ADR-0096 Decision 4)"), "{err}");
        assert!(admit(json!({ "messages": user("hi"), "stop": "" })).is_ok(), "an empty stop asks nothing");
        assert!(admit(json!({ "messages": user("hi"), "stop": null, "logit_bias": null })).is_ok());

        // ADR-0082 Decision 11's fence, by its own sentence, through the one spelling of the rule.
        let err = admit(json!({ "messages": user("hi"), "temperature": 0.7 })).unwrap_err();
        assert!(err.contains("palw_fp_decode_rules") && err.contains("SamplingNotArmed"), "{err}");
        let err = admit(json!({ "messages": user("hi"), "seed": 42 })).unwrap_err();
        assert_eq!(err, "seed must be 64 hex characters", "an OpenAI integer seed reaches the lane's rule and is refused there");
        let armed = ChainFacts { fp_decode_rules_armed: true, ..Default::default() };
        let body = serde_json::to_vec(&json!({ "messages": user("hi"), "temperature": 0.5 })).unwrap();
        let (_, admitted) = parse_and_admit(&body, &armed).expect("the armed network admits a temperature");
        assert_eq!(admitted.sampling.1, (kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_T_ONE / 2) as u32);
    }

    /// **Decision 1's first row.** Parts flatten with `\n`; a non-text part is refused with its
    /// type, its message index and its part index, and the sentence says why.
    #[test]
    fn content_parts_flatten_with_newlines_and_a_non_text_part_is_refused_by_type_and_position() {
        let parts = json!([{ "role": "system", "content": "s" }, { "role": "user", "content": [{ "type": "text", "text": "one" }, { "type": "text", "text": "two" }] }]);
        let admitted = admit(json!({ "messages": parts })).unwrap();
        assert_eq!(admitted.turns[1], ChatTurn::text("user", "one\ntwo"));
        assert_eq!(admitted.turns[0], ChatTurn::text("system", "s"));

        for (kind, needle) in [
            ("image_url", "messages[1].content[1] is a `image_url` part"),
            ("input_audio", "`input_audio` part"),
            ("file", "`file` part"),
        ] {
            let bad = json!([{ "role": "system", "content": "s" }, { "role": "user", "content": [{ "type": "text", "text": "look" }, { "type": kind, kind: {} }] }]);
            let err = admit(json!({ "messages": bad })).unwrap_err();
            assert!(err.contains(needle), "{err}");
            assert!(err.contains("this lane carries text only (ADR-0096 Decision 1)"), "{err}");
        }
        let err = admit(json!({ "messages": [{ "role": "user", "content": ["plain string part"] }] })).unwrap_err();
        assert!(err.contains("messages[0].content[0] is a string where a {type, …} part was expected"), "{err}");
        let err = admit(json!({ "messages": [{ "role": "user", "content": [{ "type": "text" }] }] })).unwrap_err();
        assert!(err.contains("text part without a `text` string"), "{err}");
        let err = admit(json!({ "messages": [{ "role": "user", "content": 7 }] })).unwrap_err();
        assert!(err.contains("messages[0].content is a number"), "{err}");
        // `null` content is an assistant turn that only made calls — admitted as empty text.
        let admitted =
            admit(json!({ "messages": [{ "role": "user", "content": "u" }, { "role": "assistant", "content": null }] })).unwrap();
        assert_eq!(admitted.turns[1].content, "");
    }

    /// **Decision 2's request side.** The round trip is admitted (calls parsed from OpenAI's
    /// argument STRING, `tool` turns kept, the ids listed as ignored), and every malformed shape
    /// is refused with the field it sits in.
    #[test]
    fn tool_turns_and_tools_are_admitted_and_their_refusals_name_the_field() {
        let round_trip = json!([
            { "role": "user", "content": "weather in Paris?" },
            { "role": "assistant", "content": null, "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "get_weather", "arguments": "{\"city\": \"Paris\"}" } }] },
            { "role": "tool", "tool_call_id": "call_1", "content": "{\"temp_c\": 21}" }
        ]);
        let admitted = admit(json!({ "messages": round_trip, "tools": [weather_tool()], "tool_choice": { "type": "function", "function": { "name": "get_weather" } } })).unwrap();
        assert_eq!(
            admitted.turns[1].tool_calls,
            vec![ToolCallTurn { name: "get_weather".into(), arguments: json!({ "city": "Paris" }) }]
        );
        assert_eq!(admitted.turns[2], ChatTurn::text("tool", "{\"temp_c\": 21}"));
        assert_eq!(admitted.tools.len(), 1);
        assert_eq!(admitted.tool_choice, ToolChoice::Named("get_weather".into()));
        assert!(admitted.tool_choice_given);
        assert_eq!(admitted.ignored_fields, vec!["messages[].tool_calls[].id", "messages[].tool_call_id"], "in encounter order");
        // Arguments as an object are read too; `none` renders nothing; absent is `auto`.
        let object_args = json!([{ "role": "user", "content": "u" }, { "role": "assistant", "content": "", "tool_calls": [{ "function": { "name": "f", "arguments": { "a": 1 } } }] }]);
        assert_eq!(admit(json!({ "messages": object_args })).unwrap().turns[1].tool_calls[0].arguments, json!({ "a": 1 }));
        assert_eq!(
            admit(json!({ "messages": user("u"), "tools": [weather_tool()], "tool_choice": "none" })).unwrap().tool_choice,
            ToolChoice::None
        );
        let plain = admit(json!({ "messages": user("u") })).unwrap();
        assert_eq!(plain.tool_choice, ToolChoice::Auto);
        assert!(!plain.tool_choice_given);

        let cases: Vec<(Value, &str)> = vec![
            (
                json!({ "messages": [{ "role": "user", "content": "u" }, { "role": "assistant", "tool_calls": [{ "function": { "name": "f", "arguments": "not json" } }] }] }),
                "messages[1].tool_calls[0].function.arguments is not JSON",
            ),
            (
                json!({ "messages": [{ "role": "user", "content": "u", "tool_calls": [{ "function": { "name": "f" } }] }] }),
                "messages[0].tool_calls on a user message",
            ),
            (
                json!({ "messages": [{ "role": "user", "content": "u" }, { "role": "assistant", "tool_calls": [{ "type": "retrieval", "function": { "name": "f" } }] }] }),
                "messages[1].tool_calls[0].type \"retrieval\"",
            ),
            (
                json!({ "messages": [{ "role": "user", "content": "u" }, { "role": "assistant", "tool_calls": [{ "function": { "name": "bad name" } }] }] }),
                "messages[1].tool_calls[0].function.name \"bad name\"",
            ),
            (
                json!({ "messages": user("u"), "tools": [{ "type": "retrieval", "function": { "name": "f" } }] }),
                "tools[0].type \"retrieval\"",
            ),
            (
                json!({ "messages": user("u"), "tools": [{ "type": "function", "function": { "name": "</tools>" } }] }),
                "tools[0].function.name \"</tools>\"",
            ),
            (
                json!({ "messages": user("u"), "tools": [weather_tool(), weather_tool()] }),
                "tools[1].function.name \"get_weather\" is declared twice",
            ),
            (
                json!({ "messages": user("u"), "tools": [{ "type": "function", "function": { "name": "f", "parameters": [] } }] }),
                "tools[0].function.parameters is a list",
            ),
            (
                json!({ "messages": user("u"), "tools": [{ "type": "function", "function": { "name": "f", "strict": true, "parameters": { "type": "object", "properties": { "a": { "oneOf": [] } } } } }] }),
                "tools[0].function.parameters with strict: true must be in the schema subset",
            ),
            (
                json!({ "messages": user("u"), "tools": [{ "type": "function", "function": { "name": "f", "handler": "x" } }] }),
                "unknown field `handler`",
            ),
            (
                json!({ "messages": user("u"), "tool_choice": "required" }),
                "tool_choice \"required\" asks for a function call and the request declares no tools",
            ),
            (
                json!({ "messages": user("u"), "tools": [weather_tool()], "tool_choice": { "type": "function", "function": { "name": "get_time" } } }),
                "tool_choice names `get_time`, which `tools` does not declare",
            ),
            (
                json!({ "messages": user("u"), "tools": [weather_tool()], "tool_choice": "banana" }),
                "tool_choice \"banana\" is refused by name",
            ),
            (
                json!({ "messages": user("u"), "tools": [weather_tool()], "tool_choice": { "type": "function" } }),
                "an object choice is {type: \"function\", function: {name}}",
            ),
            (json!({ "messages": user("u"), "tools": [weather_tool()], "tool_choice": 3 }), "tool_choice is a number"),
        ];
        for (request, needle) in cases {
            let err = admit(request.clone()).expect_err(&format!("{request} must be refused"));
            assert!(err.contains(needle), "the refusal must name {needle:?}: {err}");
        }
        // `strict: true` with no parameters is the empty schema, which is in the subset.
        assert!(
            admit(json!({ "messages": user("u"), "tools": [{ "type": "function", "function": { "name": "ping", "strict": true } }] }))
                .is_ok()
        );
    }

    /// **Decision 3 at the entrance.** `text` asks nothing; `json_object` and `json_schema` become
    /// a format whose instruction the system turn will carry; the id is over the canonical bytes;
    /// a schema outside the subset, an oversize one, an unknown type and a stray key are refused
    /// by name.
    #[test]
    fn response_format_is_admitted_inside_the_subset_and_refused_outside_it() {
        assert!(admit(json!({ "messages": user("u"), "response_format": { "type": "text" } })).unwrap().format.is_none());
        assert!(admit(json!({ "messages": user("u"), "response_format": null })).unwrap().format.is_none());
        let object = admit(json!({ "messages": user("u"), "response_format": { "type": "json_object" } })).unwrap().format.unwrap();
        assert!(matches!(object.kind, FormatKind::JsonObject));
        assert_eq!(object.constraint_id, None);
        assert_eq!(object.instruction(), "\n\nRespond with a single JSON value and nothing else.");

        let schema = json!({ "type": "object", "properties": { "name": { "type": "string" }, "age": { "type": "integer", "minimum": 0 } }, "required": ["name"], "additionalProperties": false });
        let admitted = admit(json!({ "messages": user("u"), "response_format": { "type": "json_schema", "json_schema": { "name": "person", "strict": true, "schema": schema } } })).unwrap();
        let format = admitted.format.unwrap();
        assert!(matches!(format.kind, FormatKind::JsonSchema));
        assert_eq!(format.name.as_deref(), Some("person"));
        let canonical = misaka_palw_constraint::canonical::to_rfc8785(&schema).unwrap();
        assert_eq!(format.canonical_schema.as_deref(), Some(canonical.as_slice()));
        assert_eq!(format.constraint_id, Some(misaka_palw_constraint::constraint_id(&canonical)), "the id names the canonical bytes");
        assert_eq!(
            format.instruction(),
            format!(
                "\n\nRespond with a single JSON value that conforms to this JSON Schema and nothing else:\n{}",
                String::from_utf8(canonical).unwrap()
            )
        );
        assert!(format.instruction().contains("{\"additionalProperties\":false,\"properties\":{\"age\":{\"minimum\":0,\"type\":\"integer\"},\"name\":{\"type\":\"string\"}},\"required\":[\"name\"],\"type\":\"object\"}"));

        let cases: Vec<(Value, &str)> = vec![
            (
                json!({ "type": "json_schema", "json_schema": { "schema": { "$ref": "#/x" } } }),
                "response_format.json_schema.schema: `$ref` at \"\"",
            ),
            (
                json!({ "type": "json_schema", "json_schema": { "schema": { "type": "string", "format": "email" } } }),
                "`format` at \"\" is outside the JSON-Schema subset",
            ),
            (
                json!({ "type": "json_schema", "json_schema": { "schema": { "type": "object", "description": "x".repeat(70_000) } } }),
                "bytes and the constraint cap is 65536 (ADR-0096 Decision 7)",
            ),
            (
                json!({ "type": "json_schema", "json_schema": { "schema": {}, "extra": 1 } }),
                "response_format.json_schema.extra is refused by name",
            ),
            (json!({ "type": "json_schema", "json_schema": { "name": "x" } }), "response_format.json_schema.schema is missing"),
            (json!({ "type": "json_schema" }), "response_format.json_schema is missing"),
            (json!({ "type": "xml" }), "response_format.type \"xml\" is refused by name"),
            (json!({}), "response_format has no `type`"),
            (json!("json"), "response_format is a string"),
        ];
        for (format, needle) in cases {
            let err = admit(json!({ "messages": user("u"), "response_format": format })).unwrap_err();
            assert!(err.contains(needle), "the refusal must name {needle:?}: {err}");
        }
    }

    /// **Invariant 6.** `require_committed_format: true` is refused by name while the fence is
    /// dormant — and BEFORE any worker: `admit_request` has no worker in reach (its signature is
    /// the proof), and the route calls it before the queue is reserved and before `handle_chat`
    /// is entered (pinned on the source, the way this crate pins its other orderings).
    #[test]
    fn require_committed_format_is_refused_by_name_on_a_dormant_network_and_before_any_worker() {
        let request = json!({ "messages": user("u"), "response_format": { "type": "json_object" }, "misaka": { "require_committed_format": true } });
        let err = admit(request.clone()).unwrap_err();
        assert_eq!(
            err,
            "committed format needs Params::palw_fp_decode_constraint (ADR-0096 Decision 8), which this network has not armed; ask for \
             enforcement \"advisory\" or wait for the fence"
        );
        // The flag names a guarantee this network cannot give, whether or not this request also asked for a shape.
        assert!(admit(json!({ "messages": user("u"), "misaka": { "require_committed_format": true } })).is_err());
        assert!(admit(json!({ "messages": user("u"), "misaka": { "require_committed_format": false } })).is_ok());
        let err = admit(json!({ "messages": user("u"), "misaka": { "require_commited_format": true } })).unwrap_err();
        assert!(err.contains("unknown field `require_commited_format`"), "a misspelled extension key is refused, not ignored: {err}");
        // The day the chain arms the fence, the same request is admitted — and nothing else moves.
        let armed = ChainFacts { fp_decode_constraint_armed: true, ..Default::default() };
        parse_and_admit(&serde_json::to_vec(&request).unwrap(), &armed).expect("admitted on an armed network");

        let _: fn(&ChatRequest, &ChainFacts) -> Result<AdmittedRequest, String> = admit_request;
        let source = std::include_str!("main.rs");
        let post_arm = source.find("(\"POST\", \"/v1/chat/completions\")").expect("the POST route");
        let admitted_at = source[post_arm..].find("surface::parse_and_admit(").expect("the route admits") + post_arm;
        let reserved_at = source[post_arm..].find("in_flight.fetch_add(").expect("the route reserves") + post_arm;
        let handled_at = source[post_arm..].find("handle_chat(config").expect("the route runs") + post_arm;
        assert!(admitted_at < reserved_at && reserved_at < handled_at, "admit, then reserve the queue, then touch the worker");
    }

    /// **Decision 1's refusal rows, and the doctrine's last sentence: refused by name, never
    /// dropped.** The legacy surface, more than one choice, logits, the alias conflict, a stray
    /// `stream_options` key, a field the table does not name at either level, a role this lane
    /// does not serve, and the bounds.
    #[test]
    fn the_fields_this_surface_does_not_serve_are_refused_by_name_never_dropped() {
        let cases: Vec<(Value, &str)> = vec![
            (
                json!({ "messages": user("u"), "reasoning_effort": "low" }),
                "`reasoning_effort` is not a field this surface serves (ADR-0096 Decision 1",
            ),
            (
                json!({ "messages": user("u"), "modalities": ["text"], "audio": {} }),
                "`audio`, `modalities` are not a field this surface serves",
            ),
            (
                json!({ "messages": [{ "role": "user", "content": "u", "cache_control": {} }] }),
                "messages[0] carries `cache_control` which this surface does not serve",
            ),
            (json!({ "messages": user("u"), "functions": [] }), "`functions` is OpenAI's legacy tool surface and is refused by name"),
            (json!({ "messages": user("u"), "function_call": "auto" }), "`function_call` is OpenAI's legacy tool surface"),
            (
                json!({ "messages": [{ "role": "user", "content": "u" }, { "role": "assistant", "content": "a", "function_call": { "name": "f" } }] }),
                "messages[1].function_call is OpenAI's legacy tool surface",
            ),
            (
                json!({ "messages": [{ "role": "user", "content": "u", "audio": { "id": "x" } }] }),
                "messages[0].audio is refused by name",
            ),
            (
                json!({ "messages": user("u"), "n": 2 }),
                "n 2 is refused by name (ADR-0096 Decision 1): one inference is one claim (ADR-0077 R0)",
            ),
            (json!({ "messages": user("u"), "logprobs": true }), "logprobs is refused by name"),
            (json!({ "messages": user("u"), "top_logprobs": 5 }), "top_logprobs is refused by name"),
            (
                json!({ "messages": user("u"), "max_tokens": 64, "max_completion_tokens": 128 }),
                "max_tokens 64 and max_completion_tokens 128 disagree",
            ),
            (
                json!({ "messages": user("u"), "stream": true, "stream_options": { "include_usage": true, "chunk_size": 1 } }),
                "stream_options.chunk_size is refused by name",
            ),
            (
                json!({ "messages": user("u"), "stream": true, "stream_options": { "include_usage": "yes" } }),
                "stream_options.include_usage is a string",
            ),
            (
                json!({ "messages": [{ "role": "developer", "content": "d" }, { "role": "user", "content": "u" }] }),
                "messages[0].role \"developer\" is not one of system|user|assistant|tool",
            ),
            (json!({ "messages": [{ "role": "system", "content": "s" }] }), "the request carries no user message"),
            (json!({ "messages": [] }), "the request carries no user message"),
            (json!({}), "missing field `messages`"),
        ];
        for (request, needle) in cases {
            let err = admit(request.clone()).expect_err(&format!("{request} must be refused"));
            assert!(err.contains(needle), "the refusal must name {needle:?}: {err}");
        }
        // The values that ask for nothing are admitted: one choice, no logprobs, an agreeing alias.
        let admitted = admit(json!({ "messages": user("u"), "n": 1, "logprobs": false, "top_logprobs": null, "functions": null, "max_tokens": 64, "max_completion_tokens": 64 })).unwrap();
        assert_eq!(admitted.max_tokens, Some(64));
        assert_eq!(
            admit(json!({ "messages": user("u"), "max_completion_tokens": 32 })).unwrap().max_tokens,
            Some(32),
            "the alias is read as max_tokens"
        );
        assert!(
            admit(json!({ "messages": user("u"), "stream": true, "stream_options": { "include_usage": true } }))
                .unwrap()
                .include_usage
        );
        // The message cap is the entrance's, before the worker.
        let many: Vec<Value> = (0..=crate::MAX_CHAT_MESSAGES).map(|_| json!({ "role": "user", "content": "u" })).collect();
        let err = admit(json!({ "messages": many })).unwrap_err();
        assert!(err.contains(&format!("exceeds the {}-message cap", crate::MAX_CHAT_MESSAGES)), "{err}");
    }

    /// **Decision 1's second exception.** OpenAI's no-effect fields, and the per-message fields
    /// the template does not render, are accepted and LISTED — every one by name, once. A `null`
    /// (`refusal: null`, the shape a replayed response message carries) is an absence, not a field.
    #[test]
    fn openais_no_effect_fields_are_accepted_and_listed() {
        let request = json!({
            "messages": [
                { "role": "system", "content": "s", "name": "ops" },
                { "role": "user", "content": "u", "name": "ada" },
                { "role": "assistant", "content": "a", "refusal": null, "annotations": [], "audio": null, "function_call": null }
            ],
            "user": "user-1", "metadata": { "k": "v" }, "store": false, "parallel_tool_calls": false
        });
        let admitted = admit(request).unwrap();
        assert_eq!(
            admitted.ignored_fields,
            vec!["messages[].name", "messages[].annotations", "parallel_tool_calls", "user", "metadata", "store"]
        );
        let with_refusal = admit(
            json!({ "messages": [{ "role": "user", "content": "u" }, { "role": "assistant", "content": "a", "refusal": "no" }] }),
        )
        .unwrap();
        assert_eq!(with_refusal.ignored_fields, vec!["messages[].refusal"], "a non-null value is a field, and is listed");
        assert!(admit(json!({ "messages": user("u") })).unwrap().ignored_fields.is_empty(), "nothing sent, nothing listed");
    }

    /// **Decision 3 after the run.** The shown answer is trimmed and parsed as ONE JSON value; a
    /// fence is named, trailing text is named, a schema violation is listed at its path; a valid
    /// answer carries the SHA-256 of its canonical bytes; and the report says `advisory`.
    #[test]
    fn the_format_check_reads_the_shown_answer_and_names_what_is_wrong() {
        let object = admit(json!({ "messages": user("u"), "response_format": { "type": "json_object" } })).unwrap().format.unwrap();
        let ok = object.check("  {\"a\": 1}\n");
        assert_eq!(
            ok,
            FormatReport {
                valid: true,
                errors: vec![],
                canonical_sha256: Some("015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862".into())
            }
        );
        assert_eq!(object.check("{\"a\":1}"), ok, "the digest is over canonical bytes, so spelling does not matter");
        assert!(object.check("[1, 2]").valid, "json_object is any single JSON value (ADR-0096 Decision 3)");
        let fenced = object.check("```json\n{\"a\": 1}\n```");
        assert_eq!(
            fenced,
            FormatReport { valid: false, errors: vec!["the answer is wrapped in a code fence".into()], canonical_sha256: None }
        );
        let trailing = object.check("{\"a\": 1} and that is all");
        assert!(
            !trailing.valid && trailing.errors[0].starts_with("the answer is not a single JSON value: trailing characters"),
            "{trailing:?}"
        );
        assert!(!object.check("").valid);

        let schema = json!({ "type": "object", "properties": { "name": { "type": "string", "minLength": 1 }, "age": { "type": "integer", "maximum": 150 } }, "required": ["name", "age"], "additionalProperties": false });
        let person = admit(json!({ "messages": user("u"), "response_format": { "type": "json_schema", "json_schema": { "name": "person", "schema": schema } } })).unwrap().format.unwrap();
        let ok = person.check("{\"name\": \"Ada\", \"age\": 36}");
        assert_eq!(
            ok.canonical_sha256.as_deref(),
            Some("fc234b36d9984d8c111697eae4b5315ba69588f24c68ee52a1f78c2ea5969d8f"),
            "sha256 of {{\"age\":36,\"name\":\"Ada\"}}"
        );
        let bad = person.check("{\"name\": \"\", \"age\": 200, \"x\": 1}");
        assert_eq!(
            bad.errors,
            vec![
                "at \"/age\": 200 is above the maximum 150",
                "at \"/name\": 0 characters where at least 1 were required",
                "at \"/x\": additional property `x` is not allowed"
            ]
        );
        assert!(!bad.valid && bad.canonical_sha256.is_none());

        let report = person.report_json(&ok);
        assert_eq!(report["enforcement"], json!("advisory"));
        assert_eq!(report["requested"]["type"], json!("json_schema"));
        assert_eq!(report["requested"]["name"], json!("person"));
        assert_eq!(report["requested"]["constraint_id"].as_str().map(str::len), Some(128), "a Hash64, hex");
        assert_eq!(report["valid"], json!(true));
        assert_eq!(report["errors"], json!([]));
        let object_report = object.report_json(&object.check("{}"));
        assert_eq!(
            object_report["requested"],
            json!({ "type": "json_object", "name": null, "constraint_id": null, "constraint_bytes": null })
        );
    }

    /// `GET /v1/models` names the one class, in the shape a stock client lists models with.
    #[test]
    fn the_models_list_names_the_one_class() {
        let manifest = PalwFpWorkerManifestV1 {
            version: 1,
            model_id: "Qwen/Qwen2.5-1.5B/graph-v5".into(),
            class_id: Hash64::from_u64_word(1),
            model_profile_id: Hash64::from_u64_word(2),
            runtime_manifest_hash: Hash64::from_u64_word(3),
            runtime_class_id: Hash64::from_u64_word(4),
            shape_profile_id: Hash64::from_u64_word(5),
            trace_scheme_id: Hash64::from_u64_word(6),
            tokenizer_id: Hash64::from_u64_word(7),
            n_ctx: 512,
            prefill_single_batch_cap: 512,
            vocab: 151_936,
            special_tokens: vec![("<|im_start|>".into(), 151_644), ("<|im_end|>".into(), 151_645)],
            eog_token_ids: vec![151_645],
        };
        let body = models_body("ab".repeat(64).as_str(), &manifest, crate::wire::template_id_for(&manifest), 1_757_500_000);
        assert_eq!(body["object"], json!("list"));
        assert_eq!(body["data"].as_array().map(Vec::len), Some(1), "one gateway, one class");
        let model = &body["data"][0];
        assert_eq!(model["id"], json!(MODEL_ID));
        assert_eq!(model["object"], json!("model"));
        assert_eq!(model["owned_by"], json!("misaka-palw-gateway"));
        assert_eq!(model["created"], json!(1_757_500_000));
        assert_eq!(model["misaka"]["class_id"], json!("ab".repeat(64)));
        assert_eq!(model["misaka"]["n_ctx"], json!(512));
        assert_eq!(model["misaka"]["template_id"], json!(misaka_palw_base0::chat_template::TEMPLATE_ID_CHAT_SEGMENTS_V1));
        assert_eq!(model["misaka"]["model_id"], json!("Qwen/Qwen2.5-1.5B/graph-v5"));
        assert_eq!(MODEL_ID, "misaka-palw-fp-v3");
    }

    /// SHA-256 over the corpus directory: each file's name, a NUL, its bytes, a NUL, in byte-sorted
    /// name order. The Studio pins the same value over its mirrored copy (ADR-0096 invariant 7),
    /// and §9 of the ADR records it; a corpus edit is a change to this constant in BOTH trees.
    const OPENAI_SURFACE_V1_CORPUS_SHA256: &str = "a884a871e55af023ee0adc84deababa3469826348f4f8e0eff31bc18838cc9ba";

    /// **ADR-0096 invariant 7.** Every file in `docs/openai-surface/v1/` is one request and the
    /// verdict this surface must give it, decided by the SAME function the route calls — with
    /// dormant chain facts, the state of every shipped network — and the directory's digest is
    /// what the Studio's copy must match.
    #[test]
    fn the_conformance_corpus_passes_on_this_gateway() {
        use sha2::Digest as _;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/openai-surface/v1");
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .map(|entry| (entry.file_name().to_string_lossy().into_owned(), std::fs::read(entry.path()).unwrap()))
            .collect();
        files.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        assert_eq!(files.len(), 22, "the corpus this test covered");

        let mut hasher = sha2::Sha256::new();
        let mut accepted = 0usize;
        let mut refused = 0usize;
        for (name, bytes) in &files {
            hasher.update(name.as_bytes());
            hasher.update([0u8]);
            hasher.update(bytes);
            hasher.update([0u8]);
            let case: Value = serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"));
            let request = serde_json::to_vec(&case["request"]).unwrap();
            let expect = &case["expect"];
            let outcome = parse_and_admit(&request, &dormant());
            match expect["verdict"].as_str().unwrap_or_else(|| panic!("{name}: expect.verdict")) {
                "accepted" => {
                    let (_, admitted) = outcome.unwrap_or_else(|e| panic!("{name}: expected accepted, was refused: {e}"));
                    if let Some(ignored) = expect.get("ignored_fields") {
                        // As a set: the list's order is the encounter order, which two entrances
                        // need not share; its members are the contract.
                        let mut got = admitted.ignored_fields.clone();
                        got.sort();
                        let mut want: Vec<String> =
                            serde_json::from_value(ignored.clone()).unwrap_or_else(|e| panic!("{name}: ignored_fields: {e}"));
                        want.sort();
                        assert_eq!(got, want, "{name}: misaka.ignored_fields");
                    }
                    accepted += 1;
                }
                "refused" => {
                    let err = match outcome {
                        Err(e) => e,
                        Ok(_) => panic!("{name}: expected a refusal, was accepted"),
                    };
                    let needle =
                        expect["reason_contains"].as_str().unwrap_or_else(|| panic!("{name}: a refusal needs reason_contains"));
                    assert!(err.contains(needle), "{name}: the refusal must contain {needle:?}, got: {err}");
                    refused += 1;
                }
                other => panic!("{name}: verdict {other:?}"),
            }
        }
        assert_eq!((accepted, refused), (11, 11), "the corpus is half acceptances, half refusals");
        let digest = faster_hex::hex_string(&hasher.finalize());
        assert_eq!(
            digest, OPENAI_SURFACE_V1_CORPUS_SHA256,
            "the corpus changed: update OPENAI_SURFACE_V1_CORPUS_SHA256 here, the Studio's copy of the directory and its constant, and ADR-0096 §9"
        );
    }
}
