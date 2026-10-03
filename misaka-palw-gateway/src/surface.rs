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
    /// **RFC-0001 §A.2's `penalty_window`** under llama.cpp's name: how many of the most recent
    /// GENERATED tokens the three penalties count (1..=256). Absent with a penalty active is 64.
    #[serde(default)]
    pub repeat_last_n: Option<u32>,
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
    /// **RFC-0001 §A: the job's decode controls, normalized to the canonical `DecodeConfigV4`** —
    /// `Some` exactly where the network is past `Params::palw_fp_decode_rules` (every new job is V4
    /// there, the no-op form when nothing was asked), `None` below it (a V3 job).
    pub decode: Option<kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4>,
    /// The `stop` strings, for the worker to spell with the class's tokenizer (V4 only).
    pub stop_texts: Vec<String>,
    /// Accepted fields that changed nothing, by name.
    pub ignored_fields: Vec<String>,
    pub include_usage: bool,
    /// RFC-0001 §2.4: how many candidate jobs (1: the ordinary single choice).
    pub candidates: u32,
    /// RFC-0001 §2.9: what the artifact sidecar's defaults did to this request (`None`: no sidecar).
    pub sidecar_report: Option<Value>,
    /// **RFC-0001 §2.11: the request's decoded images**, in the order the messages carried them (the V5 slot order).
    /// Decoded integer tensors only ([`crate::tensor`]); empty for a text request.
    pub images: Vec<crate::tensor::DecodedImageV1>,
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

/// The most inputs one `/v1/embeddings` request may carry (RFC-0001 §2.8). Each is a forward pass.
pub const MAX_EMBEDDING_INPUTS: usize = 16;

/// The `misaka` extension of an embeddings request: how the positions are pooled and whether the
/// vector is scaled to unit length ([`kaspa_consensus_core::palw_embedding_pool_v1`]).
#[derive(Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingsExt {
    /// `"mean"` (default) or `"last"`.
    #[serde(default)]
    pub pool: Option<String>,
    /// Scale to a unit vector in Q24 (default `true`, what an OpenAI client expects); `false`
    /// returns the pooled hidden-state codes as they are.
    #[serde(default)]
    pub normalize: Option<bool>,
    /// **RFC-0001 §2.8: ask for the embedding to be a CLAIM** (RFC-0003's `Embedding` profile). Refused by name on a
    /// gateway whose worker serves the free-prompt lane only (every build today): the job is a tensor job on a registered
    /// `Embedding` class ([`crate::tensor::embedding_claim_job_v1`]), not the local forward pass.
    #[serde(default)]
    pub claim: Option<bool>,
}

/// OpenAI's `POST /v1/embeddings` body, with this lane's own extension.
#[derive(Deserialize, Default)]
pub struct EmbeddingsRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub input: Value,
    #[serde(default)]
    pub encoding_format: Option<String>,
    #[serde(default)]
    pub dimensions: Option<Value>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub misaka: Option<EmbeddingsExt>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// What the entrance admitted of an embeddings request.
#[derive(Debug)]
pub struct AdmittedEmbeddings {
    pub inputs: Vec<String>,
    pub pool: kaspa_consensus_core::palw_embedding_pool_v1::PalwEmbeddingPoolV1,
    pub normalize: bool,
    pub ignored_fields: Vec<String>,
}

/// **Every refusal of an embeddings request, before the worker** (RFC-0001 §2.8): fields this
/// surface does not serve; an input that is not text (token-id arrays are the OpenAI form this lane
/// refuses by name — the tokenizer is the class's, not the caller's); an empty or over-long input;
/// more than [`MAX_EMBEDDING_INPUTS`] inputs; an `encoding_format` other than `float`;
/// `dimensions` (the vector is the class's hidden width); an unknown pool.
pub fn admit_embeddings(request: &EmbeddingsRequest, max_input_bytes: usize) -> Result<AdmittedEmbeddings, String> {
    if !request.extra.is_empty() {
        let names: Vec<String> = request.extra.keys().map(|k| format!("`{k}`")).collect();
        return Err(format!("{} is not a field the embeddings surface serves; refused rather than dropped", names.join(", ")));
    }
    let inputs: Vec<String> = match &request.input {
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => {
            if items.is_empty() {
                return Err("input is an empty list".to_string());
            }
            let mut texts = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match item {
                    Value::String(text) => texts.push(text.clone()),
                    other => {
                        return Err(format!(
                            "input[{i}] is {}: this surface embeds TEXT — token-id arrays are refused by name, because the tokenizer \
                             is the class's and not the caller's",
                            kind_of(other)
                        ));
                    }
                }
            }
            texts
        }
        other => return Err(format!("input is {} where a string or a list of strings was expected", kind_of(other))),
    };
    if inputs.len() > MAX_EMBEDDING_INPUTS {
        return Err(format!("{} inputs exceed the {MAX_EMBEDDING_INPUTS}-input cap (each is a forward pass)", inputs.len()));
    }
    for (i, text) in inputs.iter().enumerate() {
        if text.is_empty() {
            return Err(format!("input[{i}] is empty: an empty text has no embedding"));
        }
        if text.len() > max_input_bytes {
            return Err(format!("input[{i}] is {} bytes and the cap is {max_input_bytes}", text.len()));
        }
    }
    match request.encoding_format.as_deref() {
        None | Some("float") => {}
        Some(other) => {
            return Err(format!("encoding_format {other:?} is refused by name: only \"float\" is served (the exact Q24 integers ride `misaka`)"));
        }
    }
    if request.dimensions.as_ref().is_some_and(|v| !v.is_null()) {
        return Err("dimensions is refused by name: the vector is the class's hidden width and is not truncated".to_string());
    }
    let ext = request.misaka.clone().unwrap_or_default();
    if ext.claim == Some(true) {
        return Err("misaka.claim is refused by name: an embedding claim is an RFC-0003 `Embedding`-profile tensor job (FP job version 10) on a \
                    registered class with palw_gen_v1 armed, and this gateway's worker serves the free-prompt lane only — the pooled vector \
                    is served locally without `claim`"
            .to_string());
    }
    let pool = match ext.pool.as_deref() {
        None | Some("mean") => kaspa_consensus_core::palw_embedding_pool_v1::PalwEmbeddingPoolV1::Mean,
        Some("last") => kaspa_consensus_core::palw_embedding_pool_v1::PalwEmbeddingPoolV1::LastToken,
        Some(other) => return Err(format!("misaka.pool {other:?} is neither \"mean\" nor \"last\"")),
    };
    let mut ignored = Vec::new();
    if request.user.is_some() {
        ignored.push("user".to_string());
    }
    Ok(AdmittedEmbeddings { inputs, pool, normalize: ext.normalize.unwrap_or(true), ignored_fields: ignored })
}

/// The most candidates one request may ask for (RFC-0001 §2.4). Each is a whole job.
pub const MAX_CANDIDATES: u32 = 8;

/// **Candidate `i`'s sampling seed: `H(base_seed ‖ i)`** (RFC-0001 §2.4) — SHA-256 under a domain,
/// the 32-byte base seed, then the index as a little-endian `u32`. Every candidate, the first
/// included, takes a derived seed, so no candidate's seed is the caller's own and any two differ
/// whatever the base; and the derivation is public, so a verifier handed the base seed and `n`
/// rebuilds every job's seed.
pub fn candidate_seed_v1(base_seed: &[u8; 32], index: u32) -> [u8; 32] {
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    h.update(b"misaka.palw.fp.n-candidate-seed.v1");
    h.update(base_seed);
    h.update(index.to_le_bytes());
    h.finalize().into()
}

/// **Parse a request body and admit it** — the conformance corpus's path (the route calls
/// [`parse_and_admit_with`], which this is the no-op case of).
#[cfg(test)]
pub fn parse_and_admit(body: &[u8], facts: &ChainFacts) -> Result<(ChatRequest, AdmittedRequest), String> {
    parse_and_admit_with(body, facts, |_| ()).map(|(chat, admitted, ())| (chat, admitted))
}

/// [`parse_and_admit`] with a step between parse and admission: `prepare` may change the parsed
/// request (the artifact sidecar's generation defaults, RFC-0001 §2.9) and returns what the caller
/// wants to keep. The change is made BEFORE admission, so a default is held to every rule an
/// explicit field is.
pub fn parse_and_admit_with<R>(
    body: &[u8],
    facts: &ChainFacts,
    prepare: impl FnOnce(&mut ChatRequest) -> R,
) -> Result<(ChatRequest, AdmittedRequest, R), String> {
    let mut chat: ChatRequest = serde_json::from_slice(body).map_err(|e| format!("request body is not a chat completion: {e}"))?;
    let kept = prepare(&mut chat);
    let admitted = admit_request(&chat, facts)?;
    Ok((chat, admitted, kept))
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
    // **RFC-0001 §2.4 (P1): `n` candidates are `n` jobs** — one inference is one claim, so the
    // gateway issues `n` separate jobs for the same prompt under per-candidate seeds
    // `H(base_seed ‖ i)` ([`candidate_seed_v1`]) and returns the `choices` together. The bounds are
    // the entrance's, and each is a refusal by name: a count outside `1..=MAX_CANDIDATES`; a
    // streamed response (one SSE stream cannot interleave jobs whose commitments differ); greedy
    // decoding (`n` copies of one answer is `n` claims for the price of nothing); and a network
    // whose decode-rules fence is dormant (a seed is refused there, so no two candidates differ).
    let candidates = chat.n.unwrap_or(1);
    if candidates == 0 || candidates > MAX_CANDIDATES {
        return Err(format!(
            "n {candidates} is outside 1..={MAX_CANDIDATES} (RFC-0001 §2.4): each candidate is its own job and its own claim, so the \
             count is bounded by name"
        ));
    }
    if candidates > 1 {
        if chat.stream == Some(true) {
            return Err(format!(
                "n {candidates} with stream: true is refused by name (RFC-0001 §2.4): the candidates are separate jobs with separate \
                 commitments and cannot share one stream — request them without streaming"
            ));
        }
        if !facts.fp_decode_rules_armed {
            // The sentence is ADR-0096 Decision 1's own (the conformance corpus pins it on a dormant
            // network, where the Studio's copy reads it too), with what RFC-0001 §2.4 adds.
            return Err(format!(
                "n {candidates} is refused by name (ADR-0096 Decision 1): one inference is one claim (ADR-0077 R0), so this surface \
                 returns exactly one choice while ADR-0082 Decision 11's sampler (Params::palw_fp_decode_rules) is dormant — RFC-0001 \
                 §2.4 serves n > 1 as n jobs under derived seeds on a network that has armed it"
            ));
        }
        if chat.temperature.is_none_or(|t| t == 0.0) {
            return Err(format!(
                "n {candidates} needs temperature > 0 (RFC-0001 §2.4): at temperature 0 the {candidates} candidates are the same \
                 answer, and each would be a claim"
            ));
        }
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
    // **RFC-0001 §A: past the decode-rules fence the penalties, `logit_bias` and `stop` are rules**,
    // normalized here to the one canonical `DecodeConfigV4`; everything outside §A.2 stays refused
    // by name. Below the fence the shipped surface stands, word for word.
    let (decode, stop_texts) = if facts.fp_decode_rules_armed {
        let (decode, stop_texts) = decode_config_from_request_v1(chat, &mut sampling_requested, &mut ignored)?;
        (Some(decode), stop_texts)
    } else {
        (None, Vec::new())
    };
    let knobs = [chat.top_p, chat.top_k, chat.min_p, chat.repeat_penalty, chat.frequency_penalty, chat.presence_penalty];
    for ((name, identity), sent) in IDENTITY_KNOBS.iter().zip(knobs) {
        if decode.is_some() && matches!(*name, "repeat_penalty" | "frequency_penalty" | "presence_penalty") {
            continue;
        }
        if let Some(value) = sent {
            if value != *identity {
                return Err(if decode.is_some() {
                    format!(
                        "{name} {value} is not part of FP Job V4 (RFC-0001 §A.2 names repeat, frequency and presence penalties, logit_bias and stop): refused by name"
                    )
                } else {
                    not_a_rule(name, value)
                });
            }
            sampling_requested.insert((*name).to_string(), serde_json::json!(value));
            not_a_rule_on_this_lane.push(*name);
        }
    }
    if decode.is_none()
        && let Some(n) = chat.repeat_last_n
    {
        return Err(not_a_rule("repeat_last_n", n));
    }
    if decode.is_some() {
        // Handled above, as §A.2's controls.
    } else if let Some(bias) = &chat.logit_bias
        && !matches!(bias, Value::Null)
        && bias.as_object().is_none_or(|m| !m.is_empty())
    {
        return Err(not_a_rule("logit_bias", bias));
    }
    if decode.is_some() {
        // `stop` is §A.2's too, spelled into token ids by the worker.
    } else if let Some(stop) = &chat.stop {
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
    let mut images: Vec<crate::tensor::DecodedImageV1> = Vec::new();
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
        let content = flatten_content(i, message.content.as_ref(), &mut images)?;
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
        decode,
        stop_texts,
        ignored_fields: ignored,
        include_usage,
        candidates,
        sidecar_report: None,
        images,
    })
}

/// `messages[i].content`: a string, a list of `{type: "text"}` parts flattened with `\n`, or
/// `null`. A non-text part is refused by NAME with its position — the class's model reads token
/// ids and nothing else, and an image the entrance dropped would be a prompt the person did not
/// write.
fn flatten_content(i: usize, content: Option<&Value>, images: &mut Vec<crate::tensor::DecodedImageV1>) -> Result<String, String> {
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
                    // RFC-0001 §2.11: a decoded integer tensor, and nothing a codec would have to decode.
                    Some(crate::tensor::IMAGE_TENSOR_PART) => {
                        if images.len() >= crate::tensor::MAX_IMAGES {
                            return Err(format!(
                                "messages[{i}].content[{j}] is image {} and a job carries at most {}",
                                images.len() + 1,
                                crate::tensor::MAX_IMAGES
                            ));
                        }
                        images.push(crate::tensor::parse_image_tensor_part(&format!("messages[{i}].content[{j}]"), members)?);
                    }
                    Some(other) if crate::tensor::is_encoded_image_kind(other) => {
                        return Err(crate::tensor::refuse_encoded_image(&format!("messages[{i}].content[{j}]"), other));
                    }
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

/// **RFC-0001 §A.2: an OpenAI/llama.cpp-shaped request's decode controls, as the one canonical
/// `DecodeConfigV4`** (RFC-0001 §A, G6). Floats are mapped once, by rounding to the nearest step
/// of the fixed point (`repeat_penalty` in Q16, the rest in Q24); a range violation is refused by
/// name; entries that ask for nothing are dropped and listed (a zero `logit_bias`, a window with no
/// penalty), because the canonical form has one spelling of "nothing"; `logit_bias` is sorted by
/// token id and `stop` strings are de-duplicated. The `stop` strings travel as text: the worker
/// holds the class's tokenizer and spells each one alone (`decode_with_stop_texts_v1`).
pub fn decode_config_from_request_v1(
    chat: &ChatRequest,
    requested: &mut serde_json::Map<String, Value>,
    ignored: &mut Vec<String>,
) -> Result<(kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4, Vec<String>), String> {
    use kaspa_consensus_core::palw_decode_pipeline_v4::{
        DecodeConfigV4, PALW_DECODE_V4_MAX_BIAS_ENTRIES, PALW_DECODE_V4_MAX_STOP_SEQUENCES, PALW_DECODE_V4_Q24_ONE,
        PALW_DECODE_V4_REPEAT_Q_ONE,
    };
    let finite = |name: &str, v: f64, lo: f64, hi: f64| -> Result<f64, String> {
        if v.is_nan() || v < lo || v > hi {
            return Err(format!("{name} {v} is outside [{lo}, {hi}] (RFC-0001 §A.2)"));
        }
        Ok(v)
    };
    let mut decode = DecodeConfigV4::NOOP;
    if let Some(p) = chat.repeat_penalty {
        decode.repeat_penalty_q = (finite("repeat_penalty", p, 1.0, 4.0)? * PALW_DECODE_V4_REPEAT_Q_ONE as f64).round() as u32;
        requested.insert("repeat_penalty".into(), serde_json::json!(p));
    }
    if let Some(f) = chat.frequency_penalty {
        decode.frequency_penalty_q = (finite("frequency_penalty", f, -2.0, 2.0)? * PALW_DECODE_V4_Q24_ONE as f64).round() as i32;
        requested.insert("frequency_penalty".into(), serde_json::json!(f));
    }
    if let Some(s) = chat.presence_penalty {
        decode.presence_penalty_q = (finite("presence_penalty", s, -2.0, 2.0)? * PALW_DECODE_V4_Q24_ONE as f64).round() as i32;
        requested.insert("presence_penalty".into(), serde_json::json!(s));
    }
    if decode.penalties_active() {
        let window = chat.repeat_last_n.unwrap_or(64);
        if !(1..=256).contains(&window) {
            return Err(format!("repeat_last_n {window} is outside 1..=256 (RFC-0001 §A.2's penalty_window)"));
        }
        decode.penalty_window = window as u16;
    } else if let Some(window) = chat.repeat_last_n {
        ignored.push(format!("repeat_last_n ({window}: no penalty is active, so there is no window)"));
    }
    if let Some(bias) = &chat.logit_bias
        && !bias.is_null()
    {
        let map =
            bias.as_object().ok_or_else(|| format!("logit_bias must be an object of token id -> bias; got {}", kind_of(bias)))?;
        let mut entries: Vec<(u32, i32)> = Vec::with_capacity(map.len());
        for (key, value) in map {
            let token: u32 = key.parse().map_err(|_| format!("logit_bias key {key:?} is not a token id"))?;
            let v = value.as_f64().ok_or_else(|| format!("logit_bias[{key}] is {} where a number was expected", kind_of(value)))?;
            let q = (finite(&format!("logit_bias[{key}]"), v, -100.0, 100.0)? * PALW_DECODE_V4_Q24_ONE as f64).round() as i32;
            if q == 0 {
                ignored.push(format!("logit_bias[{key}] (a zero bias is no bias)"));
                continue;
            }
            entries.push((token, q));
        }
        entries.sort_unstable_by_key(|(token, _)| *token);
        if entries.len() > PALW_DECODE_V4_MAX_BIAS_ENTRIES {
            return Err(format!(
                "logit_bias carries {} entries; at most {PALW_DECODE_V4_MAX_BIAS_ENTRIES} (RFC-0001 §A.2)",
                entries.len()
            ));
        }
        requested.insert("logit_bias".into(), bias.clone());
        decode.logit_bias = entries;
    }
    let mut stop_texts: Vec<String> = match &chat.stop {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_str().map(str::to_string).ok_or_else(|| format!("stop[{i}] is {} where a string was expected", kind_of(v)))
            })
            .collect::<Result<_, _>>()?,
        Some(other) => return Err(format!("stop must be a string or an array of strings; got {}", kind_of(other))),
    };
    if stop_texts.iter().any(String::is_empty) {
        return Err(
            "stop carries an empty string, which would stop before any token (RFC-0001 §A.2: each sequence is 1..=16 ids)".to_string()
        );
    }
    stop_texts.sort();
    stop_texts.dedup();
    if stop_texts.len() > PALW_DECODE_V4_MAX_STOP_SEQUENCES {
        return Err(format!("stop carries {} strings; at most {PALW_DECODE_V4_MAX_STOP_SEQUENCES} (RFC-0001 §A.2)", stop_texts.len()));
    }
    if !stop_texts.is_empty() {
        requested.insert("stop".into(), serde_json::json!(stop_texts));
    }
    decode.validate_canonical().map_err(|e| format!("the decode controls are not canonical (RFC-0001 §A.2): {e}"))?;
    Ok((decode, stop_texts))
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
pub fn models_body(class_id_hex: &str, manifest: &PalwFpWorkerManifestV1, template_id: &str, created: u64, limits: Value) -> Value {
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
                // ADR-0097 Decision 2: the limits, in the list a client reads before its first
                // request — so the budget is arithmetic on the client and never a 400 it learns from.
                "limits": limits,
            },
        }],
    })
}

// ---------------------------------------------------------------------------------------------
// The limits — ADR-0097 Decision 2
// ---------------------------------------------------------------------------------------------

/// What this process was configured with, of the three bounds a request can hit before the chain
/// is consulted. Copied from `Config` at the call site rather than borrowed, because `Config` is
/// `main.rs`'s and this module is what the tests build without one.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceLimits {
    /// `--max-decode-cap` as clamped: the most decode tokens any request may ask for.
    pub max_decode_cap: u32,
    /// `--max-decode-default`: what a request that names no `max_tokens` gets.
    pub max_decode_default: u32,
    /// `--max-prompt-bytes` as clamped: the rendered prompt's byte ceiling.
    pub max_prompt_bytes: usize,
}

/// **The schema id of the limits object.** A client that pins it knows which fields to expect;
/// a new field is a new id.
pub const LIMITS_SCHEMA_V1: &str = "misaka.palw.limits.v1";

/// **`misaka.limits` — the entrance says its limits before the first token** (ADR-0097 Decision 2).
///
/// Every number a stock client needs to budget a request without a round trip that fails:
/// the class's context window (prompt and answer TOGETHER, `fp_worker.rs`'s rule), the most an
/// answer may be, the prompt's byte ceiling, the tokenizer the ids are counted in, and — for
/// each feature the OpenAI surface accepts — whether it is served, advisory, or refused on this
/// network. The enforcement words are ADR-0096 Decision 3's; the sampling words are ADR-0082
/// Decision 11's; nothing here is a promise the chain does not already make.
///
/// Served identically on `GET /v1/models` (under `misaka.limits`) and `GET /health` (`limits`),
/// so a client that reads one and an operator that reads the other see one object.
pub fn limits_body(manifest: &PalwFpWorkerManifestV1, limits: &SurfaceLimits, facts: &ChainFacts) -> Value {
    // `prompt + decode ceiling ≤ n_ctx`: an answer can never be the whole window, because at
    // least one prompt token precedes it. The gateway's cap is the operator's; the smaller wins.
    let max_output_tokens = limits.max_decode_cap.min(manifest.n_ctx.saturating_sub(1)).max(1);
    let format_enforcement = if facts.fp_decode_constraint_armed { "committed" } else { "advisory" };
    let sampling = if facts.fp_decode_rules_armed { "requested" } else { "greedy_only" };
    serde_json::json!({
        "schema": LIMITS_SCHEMA_V1,
        // The window, and the rule the worker enforces on it.
        "context_window": manifest.n_ctx,
        "prompt_plus_answer_must_fit": true,
        "prefill_single_batch_cap": manifest.prefill_single_batch_cap,
        "max_output_tokens": max_output_tokens,
        "default_output_tokens": limits.max_decode_default.min(max_output_tokens),
        "max_prompt_bytes": limits.max_prompt_bytes,
        // The ids are counted in THIS tokenizer; a client that wants to count before it sends
        // fetches the table this id names (ADR-0096 Decision 8 serves it beside the artifact).
        "tokenizer_id": faster_hex::hex_string(manifest.tokenizer_id.as_byte_slice()),
        "vocab": manifest.vocab,
        "eog_token_ids": manifest.eog_token_ids,
        // One request is one inference and one claim (ADR-0077 R0); a longer thread or answer is
        // the app's chain of requests (ADR-0096 Decision 5), never this gateway's.
        "jobs_per_request": 1,
        "streaming": true,
        "features": {
            "tools": "text_convention",
            "tool_choice": "advisory",
            "response_format": { "json_object": format_enforcement, "json_schema": format_enforcement },
            "require_committed_format": if facts.fp_decode_constraint_armed { "served" } else { "refused" },
            "sampling": { "temperature": sampling, "seed": sampling, "top_p_top_k_min_p_repeat_penalty": "identity_only" },
            "n": 1,
            "logprobs": "refused",
            "vision": "refused",
        },
        "privacy": {
            "prompt_ids_on_chain": if facts.panel_da_armed { "panel_da_available" } else { "public_da" },
            // THIS class's form (ADR-0118 Decision 3): a held class commits Merkle ids on a flat network.
            "prompt_ids_form": if facts.prompt_ids_form() == kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 {
                "merkle"
            } else {
                "flat"
            },
        },
        // What a refusal looks like, so a client can branch on a code instead of a sentence. The
        // code is where OpenAI puts one (`error.code`; `context_length_exceeded` is OpenAI's own
        // value), the numbers ride `misaka.refusal`, and the body is the same on both shapes: a
        // 400 when `stream` is false, an SSE event after the 200 head when it is true.
        "refusals": {
            "codes": ["context_length_exceeded", "prompt_bytes_exceeded"],
            "code_at": "error.code",
            "numbers_at": "misaka.refusal",
        },
    })
}

/// The worker's context refusal — `prompt N + decode ceiling M exceeds max_context_tokens C` —
/// as its three numbers, or `None` for any other message. The sentence is `fp_worker.rs`'s and
/// the Studio parses the same one (`ceiling_from_refusal`); this reads it once so a client gets
/// numbers instead of a regex.
pub fn context_refusal(message: &str) -> Option<(u64, u64, u64)> {
    let rest = message.split("prompt ").nth(1)?;
    let (prompt, rest) = rest.split_once(" + decode ceiling ")?;
    let (ceiling, rest) = rest.split_once(" exceeds max_context_tokens ")?;
    let window = rest.split(|c: char| !c.is_ascii_digit()).next()?;
    Some((prompt.trim().parse().ok()?, ceiling.trim().parse().ok()?, window.parse().ok()?))
}

/// The gateway's own prompt-bytes refusal — `the rendered prompt is B bytes and the cap is K` —
/// as its two numbers.
pub fn prompt_bytes_refusal(message: &str) -> Option<(u64, u64)> {
    let rest = message.split("the rendered prompt is ").nth(1)?;
    let (bytes, rest) = rest.split_once(" bytes and the cap is ")?;
    let cap = rest.split(|c: char| !c.is_ascii_digit()).next()?;
    Some((bytes.trim().parse().ok()?, cap.parse().ok()?))
}

/// **The gateway's error body — the one spelling** (OpenAI's shape: `{"error": {"message", "type"}}`).
///
/// `main.rs`'s `error_body` delegates here, so a refusal built by [`refusal_body`] and an error
/// built anywhere else in the gateway cannot drift apart. Every client reads `error.message`: the
/// OpenAI SDKs, and the Studio's `Lane::send` for a 400 and its SSE parser for a stream.
pub fn error_body(message: &str) -> Value {
    serde_json::json!({ "error": { "message": message, "type": "invalid_request_error" } })
}

/// **A refusal a client can branch on** (ADR-0097 Decision 2): [`error_body`], and — when the
/// sentence is one of the two bounds a request meets before the chain — `error.code` set to a
/// stable code (`context_length_exceeded` is OpenAI's own value for the same refusal, so a stock
/// SDK's `err.code` already reads it) and `misaka.refusal` carrying the numbers. It only ADDS:
/// `error.message` and `error.type` are exactly [`error_body`]'s, and any other message is
/// [`error_body`] byte for byte.
pub fn refusal_body(message: &str) -> Value {
    let refusal = if let Some((prompt_tokens, decode_ceiling, context_window)) = context_refusal(message) {
        Some(serde_json::json!({
            "code": "context_length_exceeded",
            "prompt_tokens": prompt_tokens,
            "decode_ceiling": decode_ceiling,
            "context_window": context_window,
            // What a request with THIS prompt could still ask for, or zero: the arithmetic the
            // Studio does by hand, done once here.
            "room_for_answer": context_window.saturating_sub(prompt_tokens),
        }))
    } else if let Some((prompt_bytes, cap)) = prompt_bytes_refusal(message) {
        Some(serde_json::json!({ "code": "prompt_bytes_exceeded", "prompt_bytes": prompt_bytes, "max_prompt_bytes": cap }))
    } else {
        None
    };
    let mut body = error_body(message);
    if let Some(refusal) = refusal {
        body["error"]["code"] = refusal["code"].clone();
        body["misaka"] = serde_json::json!({ "refusal": refusal });
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// **RFC-0001 §A, G6: past the decode-rules fence the API's controls become ONE canonical
    /// `DecodeConfigV4`.** Floats mapped to the fixed point by rounding; `logit_bias` sorted and its
    /// zero entry dropped (and listed); `stop` strings de-duplicated and handed to the worker as text;
    /// a penalty with no `repeat_last_n` counts 64 ids. Below the fence the same request is refused
    /// by name, word for word as the shipped surface refuses it; and past it everything §A.2 does not
    /// name is still refused by name.
    #[test]
    fn past_the_fence_the_controls_normalize_to_one_canonical_v4_config() {
        use kaspa_consensus_core::palw_decode_pipeline_v4::{DecodeConfigV4, PALW_DECODE_V4_BIAS_BAN_Q};
        let armed = ChainFacts { fp_decode_rules_armed: true, ..Default::default() };
        let admit_armed =
            |request: Value| parse_and_admit(&serde_json::to_vec(&request).unwrap(), &armed).map(|(_, admitted)| admitted);
        let request = json!({
            "messages": user("hi"), "repeat_penalty": 1.3, "frequency_penalty": 0.5, "presence_penalty": -0.5,
            "logit_bias": { "42": 5, "7": -100, "9": 0 }, "stop": ["\n\n", "END", "END"]
        });
        let admitted = admit_armed(request.clone()).expect("§A.2's controls are rules past the fence");
        let decode = admitted.decode.clone().expect("a V4 job");
        assert_eq!(
            decode,
            DecodeConfigV4 {
                repeat_penalty_q: 85_197,
                penalty_window: 64,
                frequency_penalty_q: 1 << 23,
                presence_penalty_q: -(1 << 23),
                logit_bias: vec![(7, PALW_DECODE_V4_BIAS_BAN_Q), (42, 5 << 24)],
                stop_sequences: vec![],
            }
        );
        decode.validate_canonical().expect("canonical");
        assert_eq!(admitted.stop_texts, vec!["\n\n".to_string(), "END".to_string()], "sorted, de-duplicated");
        assert!(admitted.ignored_fields.iter().any(|f| f.starts_with("logit_bias[9]")), "{:?}", admitted.ignored_fields);
        // Nothing asked is the no-op — every job past the fence is V4.
        assert_eq!(admit_armed(json!({ "messages": user("hi") })).unwrap().decode, Some(DecodeConfigV4::NOOP));
        // A window with no penalty is dropped (one spelling of "off"), a window with one is taken.
        let quiet = admit_armed(json!({ "messages": user("hi"), "repeat_last_n": 8 })).unwrap();
        assert_eq!(quiet.decode, Some(DecodeConfigV4::NOOP));
        let windowed = admit_armed(json!({ "messages": user("hi"), "presence_penalty": 1, "repeat_last_n": 8 })).unwrap();
        assert_eq!(windowed.decode.unwrap().penalty_window, 8);
        // Out of range, out of bound, or outside §A.2: refused by name.
        for (bad, needle) in [
            (json!({ "messages": user("hi"), "repeat_penalty": 4.5 }), "repeat_penalty"),
            (json!({ "messages": user("hi"), "frequency_penalty": -2.5 }), "frequency_penalty"),
            (json!({ "messages": user("hi"), "logit_bias": { "3": 101 } }), "logit_bias[3]"),
            (json!({ "messages": user("hi"), "logit_bias": { "x": 1 } }), "logit_bias key"),
            (json!({ "messages": user("hi"), "stop": ["a", "b", "c", "d", "e"] }), "stop carries 5"),
            (json!({ "messages": user("hi"), "stop": [""] }), "empty string"),
            (json!({ "messages": user("hi"), "presence_penalty": 1, "repeat_last_n": 300 }), "repeat_last_n"),
            (json!({ "messages": user("hi"), "top_p": 0.9 }), "not part of FP Job V4"),
            (json!({ "messages": user("hi"), "top_k": 40 }), "not part of FP Job V4"),
        ] {
            let err = admit_armed(bad.clone()).expect_err("refused");
            assert!(err.contains(needle), "{bad}: {err}");
        }
        let many: serde_json::Map<String, Value> = (0..301).map(|t| (t.to_string(), json!(1))).collect();
        assert!(admit_armed(json!({ "messages": user("hi"), "logit_bias": many })).unwrap_err().contains("at most 300"));
        // Below the fence: the shipped refusals, and no V4 job.
        assert!(admit(request).is_err(), "below the fence these are not rules");
        assert_eq!(admit(json!({ "messages": user("hi") })).unwrap().decode, None);
        assert!(admit(json!({ "messages": user("hi"), "repeat_last_n": 8 })).unwrap_err().contains("repeat_last_n"));
    }

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
            ("input_audio", "`input_audio` part"),
            ("file", "`file` part"),
        ] {
            let bad = json!([{ "role": "system", "content": "s" }, { "role": "user", "content": [{ "type": "text", "text": "look" }, { "type": kind, kind: {} }] }]);
            let err = admit(json!({ "messages": bad })).unwrap_err();
            assert!(err.contains(needle), "{err}");
            assert!(err.contains("this lane carries text only (ADR-0096 Decision 1)"), "{err}");
        }
        // RFC-0001 §2.11: an encoded image is refused by name with what to send; a decoded tensor is admitted.
        for kind in ["image_url", "input_image"] {
            let bad = json!([{ "role": "user", "content": [{ "type": "text", "text": "look" }, { "type": kind, kind: {} }] }]);
            let err = admit(json!({ "messages": bad })).unwrap_err();
            assert!(err.contains("messages[0].content[1]") && err.contains("decoded integer tensors only"), "{err}");
        }
        let tensor = json!({ "type": "palw_image_tensor", "h": 1, "w": 2, "format": "u8_hwc_rgb", "data": "0a0b0c0d0e0f" });
        let admitted = admit(json!({ "messages": [{ "role": "user", "content": [{ "type": "text", "text": "look" }, tensor.clone(), tensor] }] })).unwrap();
        assert_eq!(admitted.turns[0], ChatTurn::text("user", "look"), "an image part adds no text");
        assert_eq!(admitted.images.len(), 2);
        assert_eq!((admitted.images[0].h, admitted.images[0].w, admitted.images[0].rgb.as_slice()), (1, 2, &[10, 11, 12, 13, 14, 15][..]));
        let err = admit(json!({ "messages": [{ "role": "user", "content": [{ "type": "palw_image_tensor", "h": 1, "w": 2, "format": "u8_hwc_rgb", "data": "00" }] }] })).unwrap_err();
        assert!(err.contains("hex characters"), "{err}");
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
        let admitted_at = source[post_arm..].find("surface::parse_and_admit_with(").expect("the route admits") + post_arm;
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
            (json!({ "messages": user("u"), "n": 0 }), "n 0 is outside 1..=8 (RFC-0001 §2.4)"),
            (json!({ "messages": user("u"), "n": 9 }), "n 9 is outside 1..=8 (RFC-0001 §2.4)"),
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
        let limits = SurfaceLimits { max_decode_cap: 1_024, max_decode_default: 256, max_prompt_bytes: 65_536 };
        let body = models_body(
            "ab".repeat(64).as_str(),
            &manifest,
            crate::wire::template_id_for(&manifest),
            1_757_500_000,
            limits_body(&manifest, &limits, &dormant()),
        );
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
        // ADR-0097 Decision 2: the limits ride the list.
        assert_eq!(model["misaka"]["limits"]["schema"], json!(LIMITS_SCHEMA_V1));
        assert_eq!(model["misaka"]["limits"]["context_window"], json!(512));
    }

    fn a16_manifest(n_ctx: u32) -> PalwFpWorkerManifestV1 {
        PalwFpWorkerManifestV1 {
            version: 1,
            model_id: "Qwen/Qwen2.5-1.5B/graph-v5".into(),
            class_id: Hash64::from_u64_word(1),
            model_profile_id: Hash64::from_u64_word(2),
            runtime_manifest_hash: Hash64::from_u64_word(3),
            runtime_class_id: Hash64::from_u64_word(4),
            shape_profile_id: Hash64::from_u64_word(5),
            trace_scheme_id: Hash64::from_u64_word(6),
            tokenizer_id: Hash64::from_u64_word(7),
            n_ctx,
            prefill_single_batch_cap: n_ctx,
            vocab: 151_936,
            special_tokens: vec![("<|im_start|>".into(), 151_644), ("<|im_end|>".into(), 151_645)],
            eog_token_ids: vec![151_645],
        }
    }

    /// **ADR-0097 invariant 7.** The limits object says the window, the most an answer can be
    /// (never the whole window: a prompt token precedes it, and never past the operator's cap),
    /// the tokenizer, and — per feature — the word the chain's fences decide. On a dormant
    /// network the format is advisory and the sampler is greedy; arm each fence and the word moves.
    #[test]
    fn the_limits_say_the_window_the_answer_ceiling_and_what_each_fence_decides() {
        let manifest = a16_manifest(512);
        let limits = SurfaceLimits { max_decode_cap: 1_024, max_decode_default: 256, max_prompt_bytes: 65_536 };
        let body = limits_body(&manifest, &limits, &dormant());
        assert_eq!(body["schema"], json!(LIMITS_SCHEMA_V1));
        assert_eq!(body["context_window"], json!(512));
        assert_eq!(body["prompt_plus_answer_must_fit"], json!(true));
        assert_eq!(
            body["max_output_tokens"],
            json!(511),
            "the cap is 1,024 but the window is 512, and one prompt token precedes the answer"
        );
        assert_eq!(body["default_output_tokens"], json!(256));
        assert_eq!(body["max_prompt_bytes"], json!(65_536));
        assert_eq!(body["tokenizer_id"], json!(faster_hex::hex_string(Hash64::from_u64_word(7).as_byte_slice())));
        assert_eq!(body["jobs_per_request"], json!(1));
        assert_eq!(body["streaming"], json!(true));
        assert_eq!(body["features"]["response_format"]["json_schema"], json!("advisory"));
        assert_eq!(body["features"]["require_committed_format"], json!("refused"));
        assert_eq!(body["features"]["sampling"]["temperature"], json!("greedy_only"));
        assert_eq!(body["privacy"]["prompt_ids_on_chain"], json!("public_da"));
        assert_eq!(body["privacy"]["prompt_ids_form"], json!("flat"));
        // Where a refusal's code and numbers are — the places `refusal_body` actually writes.
        assert_eq!(body["refusals"]["code_at"], json!("error.code"));
        assert_eq!(body["refusals"]["numbers_at"], json!("misaka.refusal"));
        let refused = refusal_body("prompt 600 + decode ceiling 8 exceeds max_context_tokens 512");
        assert_eq!(
            refused["error"]["code"], body["refusals"]["codes"][0],
            "the first listed code is the one the window refusal carries"
        );

        // The operator's cap binds when it is the smaller number.
        let tight = SurfaceLimits { max_decode_cap: 64, max_decode_default: 256, max_prompt_bytes: 4_096 };
        let body = limits_body(&manifest, &tight, &dormant());
        assert_eq!(body["max_output_tokens"], json!(64));
        assert_eq!(body["default_output_tokens"], json!(64), "a default above the cap is the cap");

        // Each fence moves exactly its own word.
        let armed = ChainFacts {
            fp_decode_constraint_armed: true,
            fp_decode_rules_armed: true,
            panel_da_armed: true,
            prompt_ids_merkle: true,
            ..Default::default()
        };
        let body = limits_body(&manifest, &limits, &armed);
        assert_eq!(body["features"]["response_format"]["json_object"], json!("committed"));
        assert_eq!(body["features"]["require_committed_format"], json!("served"));
        assert_eq!(body["features"]["sampling"]["seed"], json!("requested"));
        assert_eq!(body["privacy"]["prompt_ids_on_chain"], json!("panel_da_available"));
        assert_eq!(body["privacy"]["prompt_ids_form"], json!("merkle"));

        // ADR-0118 Decision 3: a held class on a network minted flat reads its OWN form.
        let held_on_flat = ChainFacts { panel_da_armed: true, class_prompt_ids_merkle: true, ..Default::default() };
        let body = limits_body(&manifest, &limits, &held_on_flat);
        assert_eq!(body["privacy"]["prompt_ids_form"], json!("merkle"));
        assert!(held_on_flat.public_ids_cannot_ride() && !armed.public_ids_cannot_ride() && !dormant().public_ids_cannot_ride());
    }

    /// **ADR-0097 invariant 8.** The two refusals a request meets before the chain carry a code
    /// where OpenAI puts one (`error.code`) and their numbers under `misaka.refusal`; everything
    /// a client already read — `error.message`, `error.type` — is [`error_body`]'s exactly, and
    /// every other message is [`error_body`] byte for byte.
    ///
    /// Written after the first version of this test pinned `{"error": "<message>"}` — a string
    /// where every client reads an object — and passed, because it compared the function with
    /// itself rather than with the body the gateway already served (found by the Studio half).
    #[test]
    fn a_context_refusal_carries_a_code_and_its_numbers_and_the_body_is_the_gateways_own() {
        let worker = "the worker refused the job: prompt 51 + decode ceiling 476 exceeds max_context_tokens 512";
        assert_eq!(context_refusal(worker), Some((51, 476, 512)));
        let body = refusal_body(worker);
        assert_eq!(body["error"]["message"], json!(worker), "the sentence is where every client reads it");
        assert_eq!(body["error"]["type"], json!("invalid_request_error"));
        assert_eq!(body["error"]["code"], json!("context_length_exceeded"), "OpenAI's own code for this refusal");
        assert_eq!(body["misaka"]["refusal"]["code"], json!("context_length_exceeded"));
        assert_eq!(body["misaka"]["refusal"]["prompt_tokens"], json!(51));
        assert_eq!(body["misaka"]["refusal"]["decode_ceiling"], json!(476));
        assert_eq!(body["misaka"]["refusal"]["context_window"], json!(512));
        assert_eq!(body["misaka"]["refusal"]["room_for_answer"], json!(461));

        let bytes = "the rendered prompt is 70000 bytes and the cap is 65536 — refused before the job is sent";
        assert_eq!(prompt_bytes_refusal(bytes), Some((70_000, 65_536)));
        let body = refusal_body(bytes);
        assert_eq!(body["error"]["code"], json!("prompt_bytes_exceeded"));
        assert_eq!(body["misaka"]["refusal"]["max_prompt_bytes"], json!(65_536));

        // It only adds: remove what it added and the gateway's own body is what is left.
        for message in [worker, bytes] {
            let mut stripped = refusal_body(message);
            stripped["error"].as_object_mut().unwrap().remove("code");
            stripped.as_object_mut().unwrap().remove("misaka");
            assert_eq!(stripped, error_body(message), "{message}");
        }
        let other = "temperature 0.7 is refused by name";
        assert_eq!(refusal_body(other), error_body(other));
        assert_eq!(error_body(other), json!({ "error": { "message": other, "type": "invalid_request_error" } }));
        assert_eq!(
            context_refusal("prompt 512 + decode ceiling 8 exceeds max_context_tokens"),
            None,
            "a sentence missing its window is not the sentence"
        );
    }

    /// SHA-256 over the corpus directory: each file's name, a NUL, its bytes, a NUL, in byte-sorted
    /// name order. The Studio pins the same value over its mirrored copy (ADR-0096 invariant 7),
    /// and §9 of the ADR records it; a corpus edit is a change to this constant in BOTH trees.
    const OPENAI_SURFACE_V1_CORPUS_SHA256: &str = "5cdf928859dccdc7e841b6f4f9b01dc75a1080f3705e515ef1793cf6d08d72b9";

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

    fn admit_on(request: Value, facts: &ChainFacts) -> Result<AdmittedRequest, String> {
        parse_and_admit(&serde_json::to_vec(&request).unwrap(), facts).map(|(_, admitted)| admitted)
    }

    /// **RFC-0001 §2.4: `n` candidates, each its own job under `H(base_seed ‖ i)`.** The bounds are
    /// refusals by name — a count outside 1..=8, a stream, greedy decoding, a dormant sampler — and an
    /// armed network admits `n` with a temperature and counts it.
    #[test]
    fn n_candidates_are_admitted_where_they_can_differ_and_refused_by_name_where_they_cannot() {
        let armed = ChainFacts { fp_decode_rules_armed: true, ..Default::default() };
        let ok = admit_on(json!({ "messages": user("u"), "n": 4, "temperature": 0.8, "seed": "11".repeat(32) }), &armed).unwrap();
        assert_eq!(ok.candidates, 4);
        assert_eq!(admit_on(json!({ "messages": user("u") }), &armed).unwrap().candidates, 1);
        for (request, needle) in [
            (json!({ "messages": user("u"), "n": 4, "temperature": 0.8, "stream": true }), "n 4 with stream: true is refused by name"),
            (json!({ "messages": user("u"), "n": 4 }), "n 4 needs temperature > 0"),
            (json!({ "messages": user("u"), "n": 4, "temperature": 0.0 }), "n 4 needs temperature > 0"),
            (json!({ "messages": user("u"), "n": 9, "temperature": 0.8 }), "n 9 is outside 1..=8"),
            (json!({ "messages": user("u"), "n": 0, "temperature": 0.8 }), "n 0 is outside 1..=8"),
        ] {
            let err = admit_on(request.clone(), &armed).expect_err(&format!("{request} must be refused"));
            assert!(err.contains(needle), "{needle:?} in {err}");
        }
        let err = admit_on(json!({ "messages": user("u"), "n": 2, "temperature": 0.8 }), &dormant()).unwrap_err();
        assert!(err.contains("n 2 is refused by name (ADR-0096 Decision 1)") && err.contains("RFC-0001 §2.4"), "{err}");
    }

    #[test]
    fn candidate_seeds_are_the_domain_separated_hash_of_the_base_seed_and_the_index() {
        let base = [7u8; 32];
        let seeds: Vec<[u8; 32]> = (0..8).map(|i| candidate_seed_v1(&base, i)).collect();
        for (i, a) in seeds.iter().enumerate() {
            assert_ne!(a, &base, "no candidate runs under the caller's own seed");
            for b in &seeds[i + 1..] {
                assert_ne!(a, b, "every candidate has its own seed");
            }
        }
        assert_eq!(candidate_seed_v1(&base, 3), seeds[3], "deterministic");
        assert_ne!(candidate_seed_v1(&[8u8; 32], 3), seeds[3], "a different base is a different seed");
        // The rule is public: sha256(domain || base || u32_le(i)).
        use sha2::Digest as _;
        let mut h = sha2::Sha256::new();
        h.update(b"misaka.palw.fp.n-candidate-seed.v1");
        h.update(base);
        h.update(2u32.to_le_bytes());
        assert_eq!(<[u8; 32]>::from(h.finalize()), seeds[2]);
    }

    fn embeddings(request: Value) -> Result<AdmittedEmbeddings, String> {
        let parsed: EmbeddingsRequest = serde_json::from_value(request).map_err(|e| e.to_string())?;
        admit_embeddings(&parsed, 1024)
    }

    /// **RFC-0001 §2.8: every refusal of an embeddings request, by name, before the worker.**
    #[test]
    fn an_embeddings_request_is_admitted_as_text_and_refused_by_name_otherwise() {
        let ok = embeddings(json!({ "input": ["a", "b"], "model": "m", "user": "u", "misaka": { "pool": "last", "normalize": false } })).unwrap();
        assert_eq!((ok.inputs.len(), ok.normalize), (2, false));
        assert_eq!(ok.pool, kaspa_consensus_core::palw_embedding_pool_v1::PalwEmbeddingPoolV1::LastToken);
        assert_eq!(ok.ignored_fields, vec!["user"]);
        let default = embeddings(json!({ "input": "one text" })).unwrap();
        assert_eq!((default.inputs, default.normalize), (vec!["one text".to_string()], true));
        assert_eq!(default.pool, kaspa_consensus_core::palw_embedding_pool_v1::PalwEmbeddingPoolV1::Mean);
        for (request, needle) in [
            (json!({ "input": [1, 2, 3] }), "input[0] is a number: this surface embeds TEXT"),
            (json!({ "input": [[1, 2]] }), "token-id arrays are refused by name"),
            (json!({ "input": [] }), "input is an empty list"),
            (json!({ "input": "" }), "input[0] is empty"),
            (json!({ "input": 5 }), "input is a number where a string or a list of strings"),
            (json!({ "input": "x".repeat(2000) }), "input[0] is 2000 bytes and the cap is 1024"),
            (json!({ "input": (0..17).map(|_| "x").collect::<Vec<_>>() }), "17 inputs exceed the 16-input cap"),
            (json!({ "input": "x", "encoding_format": "base64" }), "encoding_format \"base64\" is refused by name"),
            (json!({ "input": "x", "dimensions": 64 }), "dimensions is refused by name"),
            (json!({ "input": "x", "misaka": { "pool": "max" } }), "misaka.pool \"max\" is neither"),
            (json!({ "input": "x", "misaka": { "claim": true } }), "misaka.claim is refused by name"),
            (json!({ "input": "x", "task_type": "retrieval" }), "`task_type` is not a field the embeddings surface serves"),
        ] {
            let err = embeddings(request.clone()).expect_err(&format!("{request} must be refused"));
            assert!(err.contains(needle), "{needle:?} in {err}");
        }
        assert!(embeddings(json!({ "input": "x", "encoding_format": "float", "dimensions": null })).is_ok());
    }
}
