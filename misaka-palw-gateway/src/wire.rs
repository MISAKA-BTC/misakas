//! **ADR-0077 Decisions 2 and 6, and SA-3 — the prompt the gateway built and the answer it showed,
//! checked against the ids the chain will carry.**
//!
//! Everything in this module is a pure function over bytes and ids: no process, no socket, no
//! model. That is deliberate. Decision 2's binding — *the streamed bytes are the rendering of the
//! committed output ids, or no commitment is written* — is the one place where "the user's
//! inference" and "the consensus object" are asserted to be the same run, and an assertion that
//! can only be exercised by running a 33 GiB model is an assertion nobody exercises.
//!
//! ```text
//!   messages ──▶ segments (Special ids by NAME, user text as bytes)  ──▶ the worker
//!                    │                                                      │
//!                    │ displayed prompt                    Token frames ────┤
//!                    │                                     Result frame ────┘
//!                    ▼                                          │
//!            SA-3: the specials in the committed ids ARE   W5: concat(Token.rendered)
//!            the ones this plan declared, in order              == Result.rendered
//!                    └──────────── either fails ⇒ NO COMMITMENT ─────────┘
//! ```

use std::collections::BTreeSet;

use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpPromptSegmentV1, PalwFpWorkerManifestV1, PalwFpWorkerResultV3};

// ---------------------------------------------------------------------------------------------
// The templates. A template id names an exact transform from messages to model input; editing one
// in place is a fork of the class, so a change of transform is a change of id.
// ---------------------------------------------------------------------------------------------

// Every id this gateway can run under is spelled ONCE, in `misaka_palw_base0::chat_template`,
// beside the table that maps each to its ADR-0096 `-tools` sibling: `…/chat-segments/v1` (ADR-0077
// Decision 6, ends the generation prompt at `assistant\n`), `…/chat-segments-think-closed/v1`
// (ends it with a reasoning model's own closed think block), and `…/plain-markers-segments/v1`
// (this file's own fallback, `render_plain_markers` below: ONE `Text` segment, so the user's text
// is encoded with special-token parsing DISABLED even on a model whose control tokens this gateway
// cannot name — a distinct id from the retired `…/plain-markers/v1`, which rode the `Text` arm with
// specials ENABLED: same rendered string, different ids, and ids are what consensus sees). The
// plain id used to be spelled here; it moved the day the tools table needed to name it, because
// two constants with the same value in two crates is how they stop having the same value.
// `template_id_for` below is this file's way to ask which transform a model selects.
pub use misaka_palw_base0::chat_template::TEMPLATE_ID_PLAIN_SEGMENTS_V1;

pub const MARKER_SYSTEM: &str = "### System:\n";
pub const MARKER_USER: &str = "### User:\n";
pub const MARKER_ASSISTANT: &str = "### Assistant:\n";
pub const TURN_SEPARATOR: &str = "\n\n";
/// The display-layer stop guard for the plain-marker template: the first fresh marker line ends
/// the SHOWN answer. Presentation only — the commitment covers every executed token.
pub const STOP_GUARD: &str = "\n###";

/// One chat turn as the TEMPLATE accepts it: a role in system|user|assistant and its text. A
/// request's richer messages (parts, tool calls, `tool` turns) are reduced to these by
/// [`render_tools_into_turns`] before any template sees them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub role: String,
    pub content: String,
}

/// **The prompt, as the gateway built it** — the segments it will send, the specials it declared,
/// and the string it would DISPLAY as the prompt. SA-3 is checked against this record and not
/// against a re-render, because a re-render is a second transform and two transforms are exactly
/// what "the displayed prompt and the committed ids diverge" means.
#[derive(Clone, Debug)]
pub struct PromptPlan {
    pub template_id: &'static str,
    pub segments: Vec<PalwFpPromptSegmentV1>,
    /// The control-token ids this plan placed, in the order it placed them.
    pub declared_specials: Vec<u32>,
    /// What a person reading this prompt would see.
    pub displayed: String,
}

impl PromptPlan {
    /// The plan's displayed bytes, for the length bound the entrance enforces.
    pub fn displayed_len(&self) -> usize {
        self.displayed.len()
    }
}

/// The plain-marker render — the transform the gateway has always applied, unchanged, and pinned
/// by its golden test.
pub fn render_plain_markers(messages: &[Turn]) -> Result<String, String> {
    if !messages.iter().any(|m| m.role == "user") {
        return Err("the request carries no user message".into());
    }
    let mut out = String::new();
    for message in messages {
        let marker = match message.role.as_str() {
            "system" => MARKER_SYSTEM,
            "user" => MARKER_USER,
            "assistant" => MARKER_ASSISTANT,
            other => return Err(format!("unsupported role {other:?} (system|user|assistant)")),
        };
        out.push_str(marker);
        out.push_str(&message.content);
        out.push_str(TURN_SEPARATOR);
    }
    out.push_str(MARKER_ASSISTANT);
    Ok(out)
}

/// **The template id this manifest's model selects**, without building a prompt.
///
/// The boot line and `/health` advertise the transform an answer will actually run under, so they
/// have to ASK the renderer rather than re-derive the rule. They used to re-derive it — "is
/// `<|im_start|>` declared?" — which was a second and third copy of a rule that has since grown a
/// branch, and a copy of a rule is a way for the advertised id to stop being the executed one.
pub fn template_id_for(manifest: &PalwFpWorkerManifestV1) -> &'static str {
    match misaka_palw_base0::chat_template::qwen_chat_variant_v1(&manifest.special_tokens) {
        Some(variant) => variant.template_id(),
        None => TEMPLATE_ID_PLAIN_SEGMENTS_V1,
    }
}

/// **Build the prompt** (ADR-0077 Decision 6).
///
/// ChatML when the worker's manifest declares both markers — the model then sees the template it
/// was trained on, so EOG fires and an answer ends where it ends instead of at the ceiling. The
/// plain-marker form otherwise, still as a segment so the user's text is encoded with specials
/// disabled either way: untrusted text can never smuggle a control token, which is the property
/// SA-3 then checks was actually kept.
///
/// **Which ChatML transform** is the model's own choice and not this file's: a tokenizer that
/// declares `<think>`/`</think>` is a reasoning model whose generation prompt carries a closed
/// think block, and one that does not gets exactly the bytes it always got. That rule, the
/// segments and the two template ids all live in
/// [`misaka_palw_base0::chat_template`] — this gateway has no tokenizer, and the model gates have
/// no gateway, so the ONE place both can reach is the family crate. What stays here is the
/// entrance: the role vocabulary, the plain-marker fallback, and the `PromptPlan` the SA-3 check
/// is written against.
pub fn build_prompt(manifest: &PalwFpWorkerManifestV1, messages: &[Turn]) -> Result<PromptPlan, String> {
    if !messages.iter().any(|m| m.role == "user") {
        return Err("the request carries no user message".into());
    }
    for message in messages {
        match message.role.as_str() {
            "system" | "user" | "assistant" => {}
            other => return Err(format!("unsupported role {other:?} (system|user|assistant)")),
        }
    }
    let turns: Vec<(&str, &str)> = messages.iter().map(|m| (m.role.as_str(), m.content.as_str())).collect();
    let plan = misaka_palw_base0::chat_template::qwen_chat_prompt_plan_v1(&manifest.special_tokens, &turns)
        .map_err(|e| format!("the chat template refused this prompt: {e}"))?;
    let Some(plan) = plan else {
        let rendered = render_plain_markers(messages)?;
        return Ok(PromptPlan {
            template_id: TEMPLATE_ID_PLAIN_SEGMENTS_V1,
            segments: vec![PalwFpPromptSegmentV1::Text(rendered.clone().into_bytes())],
            declared_specials: Vec::new(),
            displayed: rendered,
        });
    };
    Ok(PromptPlan {
        template_id: plan.template_id,
        segments: plan.segments,
        declared_specials: plan.declared_specials,
        displayed: plan.displayed,
    })
}

/// **[`build_prompt`] for turns that may have spoken the tool convention** (ADR-0096 Decision 2).
///
/// The segments are the same — a tool block is a `Text` segment like any other user text, and
/// SA-3's subsequence check is unchanged — but the ID is not: a prompt that carried a tool block,
/// a `<tool_call>` turn or a `<tool_response>` turn ran under a different transform from messages
/// to model input than one that did not, and two transforms under one id is the drift the ids
/// exist to prevent. `/health` keeps advertising the BASE id: that is the model's transform, read
/// off its declared tokens; the tools id is a fact about one request.
pub fn build_prompt_with_tools(manifest: &PalwFpWorkerManifestV1, tool_turns: &ToolTurns) -> Result<PromptPlan, String> {
    let mut plan = build_prompt(manifest, &tool_turns.turns)?;
    if tool_turns.spoke_tool_convention {
        plan.template_id = misaka_palw_base0::chat_template::template_id_with_tools_v1(plan.template_id).ok_or_else(|| {
            format!(
                "the template id {} has no tools sibling (ADR-0096 Decision 2): a prompt that spoke the tool convention cannot \
                 be committed under the id of one that did not, so the job is refused before the inference",
                plan.template_id
            )
        })?;
    }
    Ok(plan)
}

// ---------------------------------------------------------------------------------------------
// ADR-0096 Decision 2 — a tool call is a turn of text, in the model's own convention
// ---------------------------------------------------------------------------------------------
//
// The shipped classes' models (Qwen2.5-Instruct, Qwen3.x) were trained on the Hermes-style tool
// convention, and the lane's template is the model's (ADR-0077 Decision 6): the tool list rides the
// SYSTEM turn as `<tools>…</tools>` JSON, a call is a `<tool_call>{…}</tool_call>` block in an
// assistant turn, and a tool's reply is a `user` turn wrapping `<tool_response>…</tool_response>`.
// Everything below is TEXT — a `Text` segment, encoded with specials disabled like any other user
// text, so SA-3's subsequence check is untouched and the ids are the prompt's. The round-trip (the
// app executes the tool and sends the result back) is the app's; each leg is its own inference
// and its own claim (R0), and nothing on chain knows what a tool is.
//
// The strings are verbatim from `Qwen/Qwen2.5-1.5B-Instruct`'s `tokenizer_config.json`
// `chat_template` (read 2026-09-10). Where that template renders JSON with Python's `json.dumps`
// (`", "` and `": "` separators, the client's key order), this file renders RFC 8785 bytes: the
// client's key order and serde_json's map order are two things a prompt — and therefore a job id
// — must not be a function of, and the canonical form is one both trees can reproduce.

/// The system line placed when a request that needs a system turn (tools, a response format) has
/// none. Qwen2.5's own template writes its vendor line here and Qwen3's writes nothing; this
/// entrance and the Studio share ONE sentence so the two trees render one prompt.
pub const DEFAULT_SYSTEM_TURN: &str = "You are a helpful assistant.";

/// The head of the tool block, verbatim from the template. Each tool follows as `"\n"` + its JSON.
pub const TOOLS_BLOCK_HEAD: &str = "\n\n# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided \
                                    with function signatures within <tools></tools> XML tags:\n<tools>";
/// The tail of the tool block, verbatim from the template.
pub const TOOLS_BLOCK_TAIL: &str = "\n</tools>\n\nFor each function call, return a json object with function name and arguments within \
                                    <tool_call></tool_call> XML tags:\n<tool_call>\n{\"name\": <function-name>, \"arguments\": \
                                    <args-json-object>}\n</tool_call>";

/// One tool, as the request declared it (OpenAI's `tools[].function`).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<serde_json::Value>,
    pub strict: Option<bool>,
}

impl ToolSpec {
    /// The tool as the template renders it: OpenAI's `{"type": "function", "function": {…}}`
    /// object, in RFC 8785 bytes.
    pub fn render(&self) -> Result<String, String> {
        let mut function = serde_json::Map::new();
        function.insert("name".into(), serde_json::Value::String(self.name.clone()));
        if let Some(description) = &self.description {
            function.insert("description".into(), serde_json::Value::String(description.clone()));
        }
        if let Some(parameters) = &self.parameters {
            function.insert("parameters".into(), parameters.clone());
        }
        if let Some(strict) = self.strict {
            function.insert("strict".into(), serde_json::Value::Bool(strict));
        }
        let object = serde_json::json!({ "type": "function", "function": function });
        canonical_text(&object).map_err(|why| format!("tool `{}` has no canonical rendering: {why}", self.name))
    }
}

/// RFC 8785 bytes as a `String` — canonical JSON is UTF-8 by construction.
fn canonical_text(value: &serde_json::Value) -> Result<String, String> {
    let bytes = misaka_palw_constraint::canonical::to_rfc8785(value)?;
    String::from_utf8(bytes).map_err(|e| format!("canonical JSON is UTF-8 by construction: {e}"))
}

/// One call an assistant turn made, as the request replayed it.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCallTurn {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// One message as the entrance admitted it: a role in system|user|assistant|tool, its text, and —
/// on an assistant turn — the calls it made.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<ToolCallTurn>,
}

impl ChatTurn {
    pub fn text(role: &str, content: &str) -> Self {
        Self { role: role.to_string(), content: content.to_string(), tool_calls: Vec::new() }
    }
}

/// OpenAI's `tool_choice`, as this lane serves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolChoice {
    /// The default: the tools are offered and the model decides.
    Auto,
    /// The tools are not rendered at all.
    None,
    /// The model is TOLD it must call a function. Advisory (Decision 2): nothing masks the decode.
    Required,
    /// The model is TOLD it must call this function. Advisory, the same way.
    Named(String),
}

impl ToolChoice {
    /// Decision 2's advisory sentence — what the model is told, since on a dormant network
    /// nothing else can be done. `None` for the choices that ask nothing.
    pub fn advisory_sentence(&self) -> Option<String> {
        match self {
            Self::Auto | Self::None => None,
            Self::Required => Some("You must call a function.".to_string()),
            Self::Named(name) => Some(format!("You must call the function `{name}`.")),
        }
    }

    /// What `misaka.tool_choice.requested` reports: the choice in OpenAI's own spelling.
    pub fn requested_json(&self) -> serde_json::Value {
        match self {
            Self::Auto => serde_json::json!("auto"),
            Self::None => serde_json::json!("none"),
            Self::Required => serde_json::json!("required"),
            Self::Named(name) => serde_json::json!({ "type": "function", "function": { "name": name } }),
        }
    }
}

/// What [`render_tools_into_turns`] produced: the text turns, and whether the tool convention
/// was spoken anywhere in them — which is what decides the template id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolTurns {
    pub turns: Vec<Turn>,
    pub spoke_tool_convention: bool,
}

/// **Append text to the system turn, creating one when the request has none.**
///
/// The template treats `messages[0]` as the system turn and nothing else as one, so this reads
/// the first turn only; a system turn elsewhere is an ordinary turn to the model and stays one.
/// A created turn holds [`DEFAULT_SYSTEM_TURN`] before the appended text, so a request with tools
/// and no system line renders the line Qwen's template would (modulo the vendor's wording).
pub fn append_to_system_turn(turns: &mut Vec<Turn>, text: &str) {
    match turns.first_mut() {
        Some(first) if first.role == "system" => first.content.push_str(text),
        _ => turns.insert(0, Turn { role: "system".into(), content: format!("{DEFAULT_SYSTEM_TURN}{text}") }),
    }
}

/// **Render tool turns and the tool list into plain text turns** (ADR-0096 Decision 2), in the
/// model's own convention:
///
/// * a `tool` message becomes a `user` turn holding `<tool_response>\n…\n</tool_response>`, and a
///   RUN of them becomes ONE user turn, the responses separated by `\n` — the template's own
///   merge rule;
/// * an assistant message with `tool_calls` becomes its content (possibly empty) followed by one
///   `\n<tool_call>\n{"name": "…", "arguments": {…}}\n</tool_call>` per call, which is byte for
///   byte what the template writes after `assistant` (its leading `\n` is the one
///   `qwen_chat_prompt` writes after the role);
/// * with a non-empty `tools` list and a `tool_choice` other than `"none"`, the tool block is
///   appended to the system turn (created if absent), and `"required"` or a named function
///   appends its advisory sentence after it.
///
/// Everything else passes through unchanged. `spoke_tool_convention` is true when any of the
/// three happened, and only then does the plan carry a `-tools` template id.
pub fn render_tools_into_turns(turns: &[ChatTurn], tools: &[ToolSpec], tool_choice: &ToolChoice) -> Result<ToolTurns, String> {
    let mut out: Vec<Turn> = Vec::with_capacity(turns.len() + 1);
    let mut spoke = false;
    let mut i = 0;
    while i < turns.len() {
        let turn = &turns[i];
        match turn.role.as_str() {
            "tool" => {
                let mut responses: Vec<String> = Vec::new();
                while i < turns.len() && turns[i].role == "tool" {
                    responses.push(format!("<tool_response>\n{}\n</tool_response>", turns[i].content));
                    i += 1;
                }
                out.push(Turn { role: "user".into(), content: responses.join("\n") });
                spoke = true;
                continue;
            }
            "assistant" if !turn.tool_calls.is_empty() => {
                // The template's tail after `assistant`: `\n` + content when there is content, then
                // `\n<tool_call>…</tool_call>` per call. The renderer writes `assistant\n` itself, so
                // exactly one leading `\n` is dropped here.
                let mut tail = String::new();
                if !turn.content.is_empty() {
                    tail.push('\n');
                    tail.push_str(&turn.content);
                }
                for call in &turn.tool_calls {
                    let name = canonical_text(&serde_json::Value::String(call.name.clone()))?;
                    let arguments = canonical_text(&call.arguments)
                        .map_err(|why| format!("tool call `{}` has arguments with no canonical rendering: {why}", call.name))?;
                    tail.push_str(&format!("\n<tool_call>\n{{\"name\": {name}, \"arguments\": {arguments}}}\n</tool_call>"));
                }
                let content = tail.strip_prefix('\n').unwrap_or(&tail).to_string();
                out.push(Turn { role: "assistant".into(), content });
                spoke = true;
            }
            _ => out.push(Turn { role: turn.role.clone(), content: turn.content.clone() }),
        }
        i += 1;
    }
    if !tools.is_empty() && *tool_choice != ToolChoice::None {
        let mut block = String::from(TOOLS_BLOCK_HEAD);
        for tool in tools {
            block.push('\n');
            block.push_str(&tool.render()?);
        }
        block.push_str(TOOLS_BLOCK_TAIL);
        if let Some(sentence) = tool_choice.advisory_sentence() {
            block.push_str("\n\n");
            block.push_str(&sentence);
        }
        append_to_system_turn(&mut out, &block);
        spoke = true;
    }
    Ok(ToolTurns { turns: out, spoke_tool_convention: spoke })
}

// ---------------------------------------------------------------------------------------------
// Decision 2, the answer side — `<tool_call>` blocks become `tool_calls[]`, and nothing committed
// moves
// ---------------------------------------------------------------------------------------------

/// One call the model made, parsed out of the shown answer.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// The shown answer with its well-formed `<tool_call>` blocks lifted out.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ParsedAnswer {
    /// What remains once the parsed blocks are removed, trimmed.
    pub text: String,
    pub calls: Vec<ParsedToolCall>,
    /// Blocks that were not a `{"name", "arguments"}` object, or never closed. They stay in
    /// `text` — the model said them, and a block the entrance could not read is still the answer.
    pub unparsed_blocks: usize,
}

const TOOL_CALL_OPEN: &str = "<tool_call>";
const TOOL_CALL_CLOSE: &str = "</tool_call>";

/// **Parse every `<tool_call>…</tool_call>` block out of the shown answer.**
///
/// A pure function over the DISPLAYED text — it takes a `&str` and returns new strings, so by
/// construction it can touch neither the ids nor the bytes the commitment covers: the roots of a
/// run with and without parsing are the roots of the same run (ADR-0096 invariant 8). A block
/// whose body is not JSON with a string `name` and an `arguments` member is left where it was and
/// counted, never repaired.
pub fn parse_tool_calls(shown: &str) -> ParsedAnswer {
    let mut out = ParsedAnswer::default();
    let mut text = String::with_capacity(shown.len());
    let mut rest = shown;
    while let Some(open) = rest.find(TOOL_CALL_OPEN) {
        let after_open = &rest[open + TOOL_CALL_OPEN.len()..];
        let Some(close) = after_open.find(TOOL_CALL_CLOSE) else {
            // Never closed: the budget or the display cut ended the block. It stays as text.
            out.unparsed_blocks += 1;
            text.push_str(rest);
            rest = "";
            break;
        };
        let body = after_open[..close].trim();
        let block_end = open + TOOL_CALL_OPEN.len() + close + TOOL_CALL_CLOSE.len();
        match serde_json::from_str::<serde_json::Value>(body) {
            Ok(serde_json::Value::Object(mut members))
                if members.get("name").is_some_and(serde_json::Value::is_string) && members.contains_key("arguments") =>
            {
                let name = members.remove("name").and_then(|n| n.as_str().map(str::to_string)).expect("checked: a string");
                let arguments = members.remove("arguments").expect("checked: present");
                out.calls.push(ParsedToolCall { name, arguments });
                text.push_str(&rest[..open]);
            }
            _ => {
                out.unparsed_blocks += 1;
                text.push_str(&rest[..block_end]);
            }
        }
        rest = &rest[block_end..];
    }
    text.push_str(rest);
    out.text = text.trim().to_string();
    out
}

/// The domain the call ids are minted under. OpenAI's ids are opaque strings; this lane's are a
/// keyed hash of `(job id, index)`, so the same job names the same calls on every read and no
/// process holds a counter.
const TOOL_CALL_ID_DOMAIN: &[u8] = b"misaka-palw/tool-call-id/v1";

/// `call_` + the first 24 hex characters of `H(domain ‖ job_id ‖ index_le32)`.
pub fn tool_call_id(job_id: kaspa_hashes::Hash64, index: u32) -> String {
    let mut preimage = Vec::with_capacity(68);
    preimage.extend_from_slice(job_id.as_byte_slice());
    preimage.extend_from_slice(&index.to_le_bytes());
    let digest = kaspa_hashes::blake2b_512_keyed(TOOL_CALL_ID_DOMAIN, &preimage);
    format!("call_{}", &faster_hex::hex_string(digest.as_byte_slice())[..24])
}

/// Every id the worker's manifest calls a control token — the set no `Text` segment may ever
/// produce. Includes the EOG ids, which are control tokens whether or not the template places one.
pub fn control_token_ids(manifest: &PalwFpWorkerManifestV1) -> BTreeSet<u32> {
    manifest.special_tokens.iter().map(|(_, id)| *id).chain(manifest.eog_token_ids.iter().copied()).collect()
}

/// **ADR-0077 SA-3 — the gateway commits exactly the prompt ids it built.**
///
/// The gateway has no tokenizer, so it cannot re-derive the ids a `Text` segment produced; what it
/// CAN assert, exactly, is the structure it placed: the control tokens in the committed ids are
/// the ones this plan declared, in this order, with no extras. That is the whole of what a
/// segment-wise template buys — a control token in the committed ids that this plan did not place
/// came either from user text the worker tokenized with specials ON, or from a worker that
/// tokenized something other than the segments it was sent. Both are "the displayed prompt and the
/// committed ids diverge", and both end here with no commitment.
///
/// The text→ids step itself is bound the other way: `request_hash` covers the exact segment bytes,
/// and `validate_against_request` refuses a result that echoes a different request or whose job
/// does not bind the returned ids.
pub fn check_committed_prompt_ids(plan: &PromptPlan, committed: &[u32], control: &BTreeSet<u32>) -> Result<(), String> {
    if committed.is_empty() {
        return Err("the committed prompt carries no ids".into());
    }
    let found: Vec<u32> = committed.iter().copied().filter(|id| control.contains(id)).collect();
    if found != plan.declared_specials {
        return Err(format!(
            "SA-3: the committed prompt holds {} control tokens where this gateway placed {} — the displayed prompt and the \
             committed ids diverge, so no commitment is written (the positions are not logged: they are the prompt)",
            found.len(),
            plan.declared_specials.len()
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Decision 2 / W5 — the answer streams, the commitment does not
// ---------------------------------------------------------------------------------------------

/// **The answer as it arrives**, accumulating everything the commitment will cover and emitting
/// only what is safe to show.
///
/// Two things make the emission non-trivial and both are real: a multi-byte character straddles
/// two `Token` frames (`rendered` is that id's bytes ALONE), and the plain-marker template's stop
/// guard can straddle them too. So an incomplete UTF-8 tail is held back, and so is any suffix
/// that could still become the guard.
#[derive(Default)]
pub struct AnswerStream {
    ids: Vec<u32>,
    /// Every byte of every token, in decode order — what W5 compares. Never trimmed.
    bytes: Vec<u8>,
    /// How much of `bytes` has already gone out as an SSE delta.
    emitted: usize,
    /// Where the SHOWN answer ends. `None` while the display is still open.
    cut: Option<usize>,
    /// The display was closed by the answer itself — an EOG id or the stop guard — rather than
    /// by the run ending. A constrained run's EOG renders EMPTY (ADR-0096 §10 B4's table rule),
    /// so the cut is then at the end of the bytes and only this says the answer stopped.
    ended_by_answer: bool,
}

impl AnswerStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take one `Token` frame. Returns the delta to send, if any is safe to send yet.
    ///
    /// `eog` is the manifest's display stop: execution runs on to the declared budget (the step
    /// leaves bind the executed count before the first leaf is hashed), so an EOG id ends the
    /// SHOWN answer and nothing else.
    pub fn push(&mut self, token_id: u32, rendered: &[u8], eog: &BTreeSet<u32>) -> Option<String> {
        let before = self.bytes.len();
        self.ids.push(token_id);
        self.bytes.extend_from_slice(rendered);
        if self.cut.is_none() {
            if eog.contains(&token_id) {
                // The EOG token's own bytes are not part of the answer.
                self.cut = Some(before);
                self.ended_by_answer = true;
            } else if let Some(at) = find_guard(&self.bytes, self.emitted) {
                self.cut = Some(at);
                self.ended_by_answer = true;
            }
        }
        self.take_delta()
    }

    /// Flush whatever is displayable now that no more tokens are coming.
    pub fn finish(&mut self) -> Option<String> {
        if self.cut.is_none() {
            self.cut = Some(self.bytes.len());
        }
        self.take_delta()
    }

    fn take_delta(&mut self) -> Option<String> {
        let end = self.safe_end();
        if end <= self.emitted {
            return None;
        }
        let delta = String::from_utf8_lossy(&self.bytes[self.emitted..end]).into_owned();
        self.emitted = end;
        Some(delta)
    }

    /// The furthest byte it is safe to show right now.
    fn safe_end(&self) -> usize {
        if let Some(cut) = self.cut {
            // The display is closed: everything up to the cut may go, and nothing after it.
            return cut.max(self.emitted).min(self.bytes.len());
        }
        // Hold back anything that could still become the stop guard, then back off to a UTF-8
        // boundary so a half character never reaches a client's decoder.
        let mut end = self.bytes.len().saturating_sub(STOP_GUARD.len() - 1);
        while end > self.emitted && !is_char_boundary(&self.bytes, end) {
            end -= 1;
        }
        end.max(self.emitted)
    }

    /// The shown answer, whole. Trailing whitespace is trimmed exactly as the non-streaming path
    /// trims it, so a client that buffers the deltas and a client that asks for one response see
    /// the same answer.
    pub fn shown(&self) -> String {
        let cut = self.cut.unwrap_or(self.bytes.len()).min(self.bytes.len());
        String::from_utf8_lossy(&self.bytes[..cut]).trim_end().to_string()
    }

    pub fn ids(&self) -> &[u32] {
        &self.ids
    }

    /// Did the answer end itself (an EOG id or the stop guard) before the run's budget did?
    /// OpenAI's `finish_reason: "stop"` against `"length"`.
    pub fn ended_by_answer(&self) -> bool {
        self.ended_by_answer
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Did the worker stream at all? A `v3-job` worker emits no `Token` frames, and then there is
    /// nothing to cross-check — which the caller must say out loud rather than report as a pass.
    pub fn streamed(&self) -> bool {
        !self.ids.is_empty()
    }
}

fn is_char_boundary(bytes: &[u8], at: usize) -> bool {
    at == 0 || at >= bytes.len() || (bytes[at] & 0xC0) != 0x80
}

/// The stop guard's position at or after `from`, if the guard is complete in `bytes`.
fn find_guard(bytes: &[u8], from: usize) -> Option<usize> {
    let needle = STOP_GUARD.as_bytes();
    let start = from.saturating_sub(needle.len() - 1);
    bytes[start..].windows(needle.len()).position(|w| w == needle).map(|at| start + at)
}

/// **ADR-0077 Decision 2 / W5.** The streamed bytes are the rendering of the committed output ids,
/// or no commitment is written.
///
/// The comparison is exact and it is two-sided: the ids the stream carried are the ids the result
/// commits, and their concatenated bytes are the result's own rendering. A worker that showed one
/// answer and committed another fails here, before anything is signed — which is the whole reason
/// F1 survives a stream at all.
///
/// A worker that streamed nothing (`v3-job`) yields `Ok(false)`: there is nothing to compare, and
/// the caller reports "not streamed" rather than "checked".
pub fn check_streamed_answer(stream: &AnswerStream, result: &PalwFpWorkerResultV3) -> Result<bool, String> {
    if !stream.streamed() {
        return Ok(false);
    }
    if stream.ids() != result.output_token_ids.as_slice() {
        return Err(format!(
            "W5: the stream carried {} ids and the commitment covers {} — a worker that shows one answer and commits another \
             is not the user's inference; no commitment is written",
            stream.ids().len(),
            result.output_token_ids.len()
        ));
    }
    if stream.bytes() != result.rendered.as_slice() {
        return Err(format!(
            "W5: the streamed bytes ({}) are not the rendering of the committed ids ({}); no commitment is written",
            stream.bytes().len(),
            result.rendered.len()
        ));
    }
    Ok(true)
}

/// Trim the SHOWN answer at the stop guard — the non-streaming path's display rule, and the same
/// rule [`AnswerStream`] applies to bytes as they arrive.
pub fn display_trim(rendered: &str) -> &str {
    match rendered.find(STOP_GUARD) {
        Some(at) => rendered[..at].trim_end(),
        None => rendered.trim_end(),
    }
}

// ---------------------------------------------------------------------------------------------
// The frame wire, on a PERSISTENT stream
// ---------------------------------------------------------------------------------------------

/// One frame off a resident worker's stdout, or `None` at a clean end of stream.
///
/// Byte-for-byte the wire `kaspa_consensus_core::palw_v2::read_framed` reads — four-byte
/// little-endian length, then that many bytes — without its trailing-byte probe, which asserts one
/// frame per process and makes a resident loop impossible. A stream that ends INSIDE a frame is an
/// error and not an end: a truncated result must not read as a worker that hung up politely.
pub fn read_frame_stream<R: std::io::Read>(reader: &mut R, max_bytes: u32) -> Result<Option<Vec<u8>>, String> {
    let mut len_bytes = [0u8; 4];
    let mut filled = 0usize;
    while filled < len_bytes.len() {
        match reader.read(&mut len_bytes[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(format!("the worker stream ended {filled} bytes into a frame length")),
            Ok(n) => filled += n,
            Err(e) => return Err(format!("reading a frame length: {e}")),
        }
    }
    let len = u32::from_le_bytes(len_bytes);
    if len > max_bytes {
        return Err(format!("a worker frame of {len} bytes exceeds the {max_bytes}-byte ceiling"));
    }
    let mut payload = vec![0u8; len as usize];
    let mut filled = 0usize;
    while filled < payload.len() {
        match reader.read(&mut payload[filled..]) {
            Ok(0) => return Err(format!("the worker stream ended {filled} bytes into a {len}-byte frame")),
            Ok(n) => filled += n,
            Err(e) => return Err(format!("reading a frame body: {e}")),
        }
    }
    Ok(Some(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptJobV3};
    use kaspa_hashes::Hash64;
    use misaka_palw_base0::chat_template::{TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_V1, TEMPLATE_ID_CHAT_SEGMENTS_V1};

    fn manifest(chatml: bool) -> PalwFpWorkerManifestV1 {
        PalwFpWorkerManifestV1 {
            version: 1,
            model_id: "Qwen/test/graph-v3".into(),
            class_id: Hash64::from_u64_word(1),
            model_profile_id: Hash64::from_u64_word(2),
            runtime_manifest_hash: Hash64::from_u64_word(3),
            runtime_class_id: Hash64::from_u64_word(4),
            shape_profile_id: Hash64::from_u64_word(5),
            trace_scheme_id: Hash64::from_u64_word(6),
            tokenizer_id: Hash64::from_u64_word(7),
            n_ctx: 512,
            prefill_single_batch_cap: 512,
            vocab: 152_000,
            special_tokens: if chatml {
                vec![("<|im_start|>".into(), 151_644), ("<|im_end|>".into(), 151_645), ("<|endoftext|>".into(), 151_643)]
            } else {
                Vec::new()
            },
            eog_token_ids: if chatml { vec![151_645, 151_643] } else { vec![151_643] },
        }
    }

    fn turns(pairs: &[(&str, &str)]) -> Vec<Turn> {
        pairs.iter().map(|(r, c)| Turn { role: (*r).into(), content: (*c).into() }).collect()
    }

    /// A reasoning model's manifest: the QWEN36 lane's own table, MEASURED from
    /// `Qwen3.5-2B-Q4_K_M.gguf` — the two ChatML markers plus the two think markers, at the ids
    /// that file declares.
    fn think_manifest() -> PalwFpWorkerManifestV1 {
        PalwFpWorkerManifestV1 {
            model_id: "Qwen/Qwen3.5-2B/graph-v3".into(),
            vocab: 248_320,
            special_tokens: vec![
                ("<|endoftext|>".into(), 248_044),
                ("<|im_start|>".into(), 248_045),
                ("<|im_end|>".into(), 248_046),
                ("<think>".into(), 248_068),
                ("</think>".into(), 248_069),
            ],
            eog_token_ids: vec![248_046, 248_044],
            ..manifest(true)
        }
    }

    /// **The QWEN36 lane's generation prompt is the model's own** — and the id says so.
    ///
    /// The defect this closes: the shipped assembly stopped at `assistant\n`, which no branch of
    /// `Qwen3.5-2B-Q4_K_M.gguf`'s `tokenizer.chat_template` does, so the model emitted the think
    /// block into the ANSWER and `grammar.canonicalize` refused at column 1.
    #[test]
    fn a_reasoning_models_prompt_carries_its_closed_think_block() {
        let m = think_manifest();
        let plan = build_prompt(&m, &turns(&[("user", "hi")])).unwrap();
        assert_eq!(plan.template_id, TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_V1);
        assert_eq!(plan.template_id, template_id_for(&m), "the id /health advertises is the id build_prompt used");
        assert_ne!(plan.template_id, TEMPLATE_ID_CHAT_SEGMENTS_V1, "an old and a new prompt must never carry the same id");
        assert_eq!(plan.displayed, "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n");
        assert_eq!(plan.declared_specials, vec![248_045, 248_046, 248_045, 248_068, 248_069]);
        // The think markers are ids, not text: spelled as text they are a MISSPELLED think block
        // (ADR-0079 Decision 7 encodes every `Text` segment with specials disabled).
        assert!(plan.segments.contains(&PalwFpPromptSegmentV1::Special(248_068)));
        assert!(plan.segments.contains(&PalwFpPromptSegmentV1::Special(248_069)));
        for segment in &plan.segments {
            if let PalwFpPromptSegmentV1::Text(bytes) = segment {
                let text = String::from_utf8(bytes.clone()).unwrap();
                assert!(!text.contains("think"), "a think marker rode inside a text segment: {text:?}");
            }
        }
        // SA-3 still holds over the longer declaration: the five ids this plan placed, in order.
        let control = control_token_ids(&m);
        let honest: Vec<u32> = vec![248_045, 10, 11, 248_046, 12, 248_045, 13, 248_068, 14, 248_069, 15];
        check_committed_prompt_ids(&plan, &honest, &control).expect("the plan's own specials, in order");
        let missing_preamble: Vec<u32> = vec![248_045, 10, 11, 248_046, 12, 248_045, 13];
        assert!(
            check_committed_prompt_ids(&plan, &missing_preamble, &control).is_err(),
            "a worker that dropped the preamble is a worker that ran a different prompt"
        );
    }

    /// **The dense/A16 lane does not move.** Not a claim about the variant — the whole plan, and
    /// against the table the dense tier actually declares: all 22 added tokens of the shipped
    /// `models/qwen2.5-1.5b/tokenizer.json`, of which NEITHER is a think marker. Prompt ids are a
    /// function of (segments, tokenizer); the tokenizer did not change, so identical segments are
    /// identical ids, and the tier whose SMF and STL evidence was measured under this exact
    /// assembly keeps it.
    #[test]
    fn the_dense_lane_is_byte_for_byte_the_transform_it_was_measured_under() {
        let dense_names = [
            ("<|endoftext|>", 151_643u32),
            ("<|im_start|>", 151_644),
            ("<|im_end|>", 151_645),
            ("<|object_ref_start|>", 151_646),
            ("<|object_ref_end|>", 151_647),
            ("<|box_start|>", 151_648),
            ("<|box_end|>", 151_649),
            ("<|quad_start|>", 151_650),
            ("<|quad_end|>", 151_651),
            ("<|vision_start|>", 151_652),
            ("<|vision_end|>", 151_653),
            ("<|vision_pad|>", 151_654),
            ("<|image_pad|>", 151_655),
            ("<|video_pad|>", 151_656),
            ("<tool_call>", 151_657),
            ("</tool_call>", 151_658),
            ("<|fim_prefix|>", 151_659),
            ("<|fim_middle|>", 151_660),
            ("<|fim_suffix|>", 151_661),
            ("<|fim_pad|>", 151_662),
            ("<|repo_name|>", 151_663),
            ("<|file_sep|>", 151_664),
        ];
        assert_eq!(dense_names.len(), 22, "the dense tokenizer.json declares 22 added tokens; this list is that list");
        assert!(
            !dense_names.iter().any(|(n, _)| *n == "<think>" || *n == "</think>"),
            "checked: the dense table declares neither think marker, which is why it cannot select the reasoning transform"
        );
        let m = PalwFpWorkerManifestV1 {
            model_id: "Qwen/Qwen2.5-1.5B/graph-v2".into(),
            vocab: 151_936,
            special_tokens: dense_names.iter().map(|(n, id)| ((*n).to_string(), *id)).collect(),
            eog_token_ids: vec![151_645, 151_643],
            ..manifest(true)
        };
        assert_eq!(template_id_for(&m), TEMPLATE_ID_CHAT_SEGMENTS_V1);
        let plan = build_prompt(&m, &turns(&[("user", "Emit the minimal cad/v1 DSL for a box. Output only JSON.")])).unwrap();
        assert_eq!(plan.template_id, TEMPLATE_ID_CHAT_SEGMENTS_V1);
        assert_eq!(plan.declared_specials, vec![151_644, 151_645, 151_644]);
        assert_eq!(
            plan.segments,
            vec![
                PalwFpPromptSegmentV1::Special(151_644),
                PalwFpPromptSegmentV1::Text(b"user\nEmit the minimal cad/v1 DSL for a box. Output only JSON.".to_vec()),
                PalwFpPromptSegmentV1::Special(151_645),
                PalwFpPromptSegmentV1::Text(b"\n".to_vec()),
                PalwFpPromptSegmentV1::Special(151_644),
                PalwFpPromptSegmentV1::Text(b"assistant\n".to_vec()),
            ],
            "the dense lane's segments must be the ones its evidence was measured against"
        );
        assert_eq!(
            plan.displayed,
            "<|im_start|>user\nEmit the minimal cad/v1 DSL for a box. Output only JSON.<|im_end|>\n<|im_start|>assistant\n"
        );
        assert!(!plan.displayed.contains("think"));
    }

    /// **The two spellings of this template cannot drift.** The gateway's assembly and
    /// `misaka_palw_base0::tokenizer::qwen_chat_prompt` are built by unrelated code, and the
    /// builder compares them on every call — this test is the one that names the corpus that
    /// comparison ran over, so the check's coverage is readable and not merely asserted.
    #[test]
    fn the_gateways_assembly_is_this_trees_other_spelling_of_the_same_template() {
        use misaka_palw_base0::chat_template::qwen35_chat_prompt;
        use misaka_palw_base0::tokenizer::qwen_chat_prompt;
        let corpus: [&[(&str, &str)]; 5] = [
            &[("user", "")],
            &[("user", "hi")],
            &[("system", "You are helpful."), ("user", "Hi")],
            &[("user", "字 — a multi-byte turn")],
            &[("user", "hi"), ("assistant", "hello"), ("user", "bye")],
        ];
        let mut checked = 0usize;
        for pairs in corpus {
            let dense = build_prompt(&manifest(true), &turns(pairs)).unwrap();
            assert_eq!(dense.displayed, qwen_chat_prompt(None, pairs), "dense spelling diverged on {pairs:?}");
            let think = build_prompt(&think_manifest(), &turns(pairs)).unwrap();
            assert_eq!(think.displayed, qwen35_chat_prompt(None, pairs), "reasoning spelling diverged on {pairs:?}");
            assert_eq!(think.displayed, format!("{}<think>\n\n</think>\n\n", dense.displayed));
            checked += 1;
        }
        assert_eq!(checked, 5, "the corpus this check covered: 5 message lists, both lanes each");
    }

    /// **The plain-marker transform is frozen** (its id names this exact transform).
    #[test]
    fn the_plain_marker_render_is_frozen() {
        let rendered = render_plain_markers(&turns(&[("system", "You are a concise assistant."), ("user", "What is 2+2?")])).unwrap();
        assert_eq!(rendered, "### System:\nYou are a concise assistant.\n\n### User:\nWhat is 2+2?\n\n### Assistant:\n");
        let multi = render_plain_markers(&turns(&[("user", "hi"), ("assistant", "hello"), ("user", "bye")])).unwrap();
        assert_eq!(multi, "### User:\nhi\n\n### Assistant:\nhello\n\n### User:\nbye\n\n### Assistant:\n");
        assert!(render_plain_markers(&turns(&[("system", "s")])).is_err(), "no user message is not a chat");
        assert!(render_plain_markers(&turns(&[("tool", "x"), ("user", "u")])).is_err(), "unknown roles are refused, not dropped");
    }

    /// **ADR-0077 Decision 6.** With the control tokens declared, the markers are `Special` ids
    /// looked up by NAME and the user's words are `Text`. Without them the same messages still
    /// travel as a segment — so user text is encoded with specials disabled on every model — and
    /// the template id says which transform ran.
    #[test]
    fn the_prompt_is_segments_and_the_markers_are_never_user_text() {
        let plan = build_prompt(&manifest(true), &turns(&[("user", "hi")])).unwrap();
        assert_eq!(plan.template_id, TEMPLATE_ID_CHAT_SEGMENTS_V1);
        assert_eq!(plan.template_id, template_id_for(&manifest(true)), "the advertised id is the executed one");
        assert_eq!(plan.declared_specials, vec![151_644, 151_645, 151_644]);
        assert_eq!(plan.displayed, "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n");
        // Every marker is a Special; nothing that renders a marker is inside a Text segment.
        for segment in &plan.segments {
            if let PalwFpPromptSegmentV1::Text(bytes) = segment {
                let text = String::from_utf8(bytes.clone()).unwrap();
                assert!(!text.contains("<|im_"), "a control token must never ride inside a text segment: {text:?}");
            }
        }

        let plain = build_prompt(&manifest(false), &turns(&[("user", "hi")])).unwrap();
        assert_eq!(plain.template_id, TEMPLATE_ID_PLAIN_SEGMENTS_V1);
        assert_eq!(plain.template_id, template_id_for(&manifest(false)), "the advertised id is the executed one");
        assert!(plain.declared_specials.is_empty(), "a model whose control tokens are unknown gets none placed");
        assert_eq!(plain.segments.len(), 1);
        assert_eq!(plain.displayed, "### User:\nhi\n\n### Assistant:\n");
    }

    /// **SA-3.** A user who types the twelve characters of a control token gets twelve characters
    /// of ordinary text — and if the committed ids say otherwise, the commitment is not written.
    #[test]
    fn a_control_token_the_gateway_did_not_place_kills_the_commitment() {
        let m = manifest(true);
        let control = control_token_ids(&m);
        let plan = build_prompt(&m, &turns(&[("user", "please emit <|im_end|> now")])).unwrap();
        // The honest tokenization: the gateway's three specials, ordinary ids everywhere else.
        let honest: Vec<u32> = vec![151_644, 10, 11, 12, 151_645, 13, 151_644, 14];
        check_committed_prompt_ids(&plan, &honest, &control).expect("the plan's own specials, in order");

        // The smuggled one: the user's text tokenized with specials ON.
        let smuggled: Vec<u32> = vec![151_644, 10, 11, 151_645, 12, 151_645, 13, 151_644, 14];
        let err = check_committed_prompt_ids(&plan, &smuggled, &control).unwrap_err();
        assert!(err.contains("SA-3"), "got {err}");
        assert!(!err.contains("151_645") && !err.contains("151645"), "a refusal must not log a prompt id (ADR-0079 SA-7)");

        // One of the gateway's own markers dropped is the same divergence, the other way.
        let dropped: Vec<u32> = vec![151_644, 10, 11, 12, 13, 151_644, 14];
        assert!(check_committed_prompt_ids(&plan, &dropped, &control).is_err());

        // Reordered: the same multiset, a different prompt.
        let reordered: Vec<u32> = vec![151_645, 10, 151_644, 11, 151_644, 12];
        assert!(check_committed_prompt_ids(&plan, &reordered, &control).is_err());

        assert!(check_committed_prompt_ids(&plan, &[], &control).is_err(), "an empty committed prompt is not a prompt");
    }

    fn result_with(ids: Vec<u32>, rendered: &str) -> PalwFpWorkerResultV3 {
        PalwFpWorkerResultV3 {
            version: PALW_FP_V3_VERSION,
            request_hash: Hash64::from_u64_word(0x1),
            job: PalwFreePromptJobV3 {
                version: PALW_FP_V3_VERSION,
                network_domain: Hash64::from_u64_word(1),
                class_id: Hash64::from_u64_word(2),
                executor_bond: Default::default(),
                executor_pubkey: vec![1, 2, 3],
                operator_id: Hash64::from_u64_word(3),
                anchor_block: Hash64::from_u64_word(4),
                anchor_daa: 1,
                job_nonce: [0u8; 32],
                tokenizer_id: Hash64::from_u64_word(5),
                prompt_token_ids_hash: Hash64::from_u64_word(6),
                prompt_tokens: 3,
                decode_token_limit: ids.len() as u32,
                max_context_tokens: 512,
                privacy_mode: 0,
                prompt_mode: 0,
                sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
                temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
                constraint_id: Default::default(),
            },
            prompt_token_ids: vec![1, 2, 3],
            trace_root: Hash64::from_u64_word(7),
            output_root: Hash64::from_u64_word(8),
            schedule_root: Hash64::from_u64_word(9),
            execution_root: Hash64::from_u64_word(10),
            trace_manifest_root: Hash64::from_u64_word(11),
            trace_chunk_count: 1,
            trace_event_count: ids.len() as u32,
            decode_tokens_executed: ids.len() as u32,
            step_leaf_count: 128,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            output_token_ids: ids,
            rendered: rendered.as_bytes().to_vec(),
            model_load_ms: 1,
            execute_ms: 1,
        }
    }

    /// **W5 with teeth.** The concatenated `Token` bytes ARE the result's rendering and the
    /// streamed ids ARE the committed ids — or the check fails and no commitment is written.
    #[test]
    fn the_streamed_answer_must_be_the_committed_one() {
        let eog: BTreeSet<u32> = BTreeSet::new();
        let mut stream = AnswerStream::new();
        for (id, bytes) in [(1u32, "2+2"), (2, " is"), (3, " 4.")] {
            stream.push(id, bytes.as_bytes(), &eog);
        }
        stream.finish();
        assert!(check_streamed_answer(&stream, &result_with(vec![1, 2, 3], "2+2 is 4.")).unwrap(), "an honest run checks out");

        // The bytes shown are not the bytes committed.
        let err = check_streamed_answer(&stream, &result_with(vec![1, 2, 3], "2+2 is 5.")).unwrap_err();
        assert!(err.contains("W5"), "got {err}");
        // The ids shown are not the ids committed.
        let err = check_streamed_answer(&stream, &result_with(vec![1, 2, 3, 4], "2+2 is 4.")).unwrap_err();
        assert!(err.contains("W5"), "got {err}");

        // A worker that streamed nothing is reported as not streamed, never as checked.
        let silent = AnswerStream::new();
        assert!(!check_streamed_answer(&silent, &result_with(vec![1], "x")).unwrap());
    }

    /// A multi-byte character straddles two `Token` frames, so nothing goes out until the
    /// character is whole — a client's decoder must never see half of one.
    #[test]
    fn an_incomplete_utf8_tail_is_held_back() {
        let eog: BTreeSet<u32> = BTreeSet::new();
        let mut stream = AnswerStream::new();
        // "日" is E6 97 A5, split across two frames, then enough tail to clear the guard hold-back.
        assert_eq!(stream.push(1, &[0xE6, 0x97], &eog), None, "half a character is never emitted");
        stream.push(2, &[0xA5], &eog);
        stream.push(3, "ほんじつ".as_bytes(), &eog);
        let all: String = std::iter::from_fn(|| stream.finish()).collect();
        assert!(!all.contains('\u{FFFD}'), "no replacement character ever reaches a client: {all:?}");
        assert_eq!(stream.shown(), "日ほんじつ");
        assert_eq!(stream.bytes(), "日ほんじつ".as_bytes());
    }

    /// The display stops at an EOG id and at the stop guard; the COMMITMENT keeps every executed
    /// token either way, which is what makes the stop a display rule rather than a budget.
    #[test]
    fn the_display_stops_but_the_capture_does_not() {
        let eog: BTreeSet<u32> = [151_645u32].into_iter().collect();
        let mut stream = AnswerStream::new();
        stream.push(1, b"four.", &eog);
        stream.push(151_645, b"<|im_end|>", &eog);
        stream.push(2, b" and more the model kept generating", &eog);
        stream.finish();
        assert_eq!(stream.shown(), "four.", "an EOG id ends the SHOWN answer and its own bytes are not shown");
        assert_eq!(stream.ids().len(), 3, "every executed token is still captured");
        assert_eq!(stream.bytes(), b"four.<|im_end|> and more the model kept generating");

        // The plain-marker guard, split across two frames.
        let none: BTreeSet<u32> = BTreeSet::new();
        let mut guarded = AnswerStream::new();
        guarded.push(1, b"2+2=4.\n", &none);
        guarded.push(2, b"##", &none);
        guarded.push(3, b"# User:\nnext", &none);
        guarded.finish();
        assert_eq!(guarded.shown(), "2+2=4.");
        assert_eq!(display_trim("2+2=4.\n\n### User:\nWhat…"), "2+2=4.", "and the non-streaming rule agrees");
    }

    /// A masked run's EOG renders EMPTY (ADR-0096 §10 B4), so the shown answer and the bytes are
    /// the same length and only the stream can say the answer stopped itself. Measured on the
    /// first masked answer: `{"name":null,"age":null}` then EOG to the budget reported
    /// `finish_reason: "length"`.
    #[test]
    fn an_empty_eog_still_ends_the_answer() {
        let eog: BTreeSet<u32> = [151_643u32, 151_645].into_iter().collect();
        let mut stream = AnswerStream::new();
        stream.push(4913, b"{\"", &eog);
        stream.push(64, b"a\":1}", &eog);
        assert!(!stream.ended_by_answer(), "still open");
        for _ in 0..3 {
            stream.push(151_643, b"", &eog);
        }
        stream.finish();
        assert!(stream.ended_by_answer());
        assert_eq!(stream.shown(), "{\"a\":1}");
        assert_eq!(stream.shown().len(), stream.bytes().len(), "the byte comparison sees no cut");

        // The run's budget ending the display is not the answer ending it.
        let mut cut_by_budget = AnswerStream::new();
        cut_by_budget.push(1, b"{\"a\":", &eog);
        cut_by_budget.finish();
        assert!(!cut_by_budget.ended_by_answer());
    }

    /// What a client that concatenates the deltas sees is what a client that asks for one response
    /// sees. Two answers to "what did the model say" is exactly the split F1 forbids.
    #[test]
    fn the_deltas_concatenate_to_the_shown_answer() {
        let eog: BTreeSet<u32> = BTreeSet::new();
        let mut stream = AnswerStream::new();
        let mut deltas = String::new();
        for (id, piece) in [(1u32, "The "), (2, "answer "), (3, "is "), (4, "four."), (5, "\n\n### User:")] {
            if let Some(delta) = stream.push(id, piece.as_bytes(), &eog) {
                deltas.push_str(&delta);
            }
        }
        if let Some(delta) = stream.finish() {
            deltas.push_str(&delta);
        }
        assert_eq!(deltas.trim_end(), stream.shown());
        assert_eq!(stream.shown(), "The answer is four.");
    }

    // -----------------------------------------------------------------------------------------
    // ADR-0096 Decision 2 — tools as the model's own text
    // -----------------------------------------------------------------------------------------

    fn weather_tool() -> ToolSpec {
        ToolSpec {
            name: "get_weather".into(),
            description: Some("Get the weather".into()),
            parameters: Some(
                serde_json::json!({ "type": "object", "properties": { "city": { "type": "string" } }, "required": ["city"] }),
            ),
            strict: None,
        }
    }

    /// The tool block, byte for byte, is Qwen2.5-Instruct's own `chat_template` text with the
    /// tool JSON in RFC 8785 form — and a request with no system turn gets one that holds the
    /// shared default line and the block.
    #[test]
    fn the_tool_block_is_the_models_own_template_text() {
        let rendered =
            render_tools_into_turns(&[ChatTurn::text("user", "weather in Paris?")], &[weather_tool()], &ToolChoice::Auto).unwrap();
        assert!(rendered.spoke_tool_convention);
        assert_eq!(rendered.turns.len(), 2, "a system turn was created");
        assert_eq!(rendered.turns[0].role, "system");
        assert_eq!(
            rendered.turns[0].content,
            "You are a helpful assistant.\n\n# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided \
             with function signatures within <tools></tools> XML tags:\n<tools>\n\
             {\"function\":{\"description\":\"Get the weather\",\"name\":\"get_weather\",\"parameters\":{\"properties\":{\"city\":{\"type\":\"string\"}},\"required\":[\"city\"],\"type\":\"object\"}},\"type\":\"function\"}\n\
             </tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> \
             XML tags:\n<tool_call>\n{\"name\": <function-name>, \"arguments\": <args-json-object>}\n</tool_call>"
        );
        assert_eq!(rendered.turns[1], Turn { role: "user".into(), content: "weather in Paris?".into() });

        // An existing system turn is appended to, never duplicated; two tools are two lines.
        let two = render_tools_into_turns(
            &[ChatTurn::text("system", "Be brief."), ChatTurn::text("user", "hi")],
            &[weather_tool(), ToolSpec { name: "get_time".into(), description: None, parameters: None, strict: Some(true) }],
            &ToolChoice::Auto,
        )
        .unwrap();
        assert_eq!(two.turns.len(), 2);
        assert!(two.turns[0].content.starts_with("Be brief.\n\n# Tools\n\n"));
        assert!(two.turns[0].content.contains("\n<tools>\n{\"function\":{\"description\":\"Get the weather\""));
        assert!(
            two.turns[0]
                .content
                .contains("}\n{\"function\":{\"name\":\"get_time\",\"strict\":true},\"type\":\"function\"}\n</tools>\n")
        );

        // `tool_choice: "none"` renders no block and speaks no convention: the plain transform.
        let none = render_tools_into_turns(&[ChatTurn::text("user", "hi")], &[weather_tool()], &ToolChoice::None).unwrap();
        assert_eq!(none, ToolTurns { turns: vec![Turn { role: "user".into(), content: "hi".into() }], spoke_tool_convention: false });
        // And no tools at all is the identity on plain turns.
        let plain =
            render_tools_into_turns(&[ChatTurn::text("system", "s"), ChatTurn::text("user", "u")], &[], &ToolChoice::Auto).unwrap();
        assert!(!plain.spoke_tool_convention);
        assert_eq!(plain.turns, turns(&[("system", "s"), ("user", "u")]));
    }

    /// `required` and a named function are ADVISORY: one sentence after the block, and nothing
    /// else — a decode mask is Part B's, and the report says `advisory`.
    #[test]
    fn tool_choice_required_and_named_are_one_advisory_sentence() {
        let required = render_tools_into_turns(&[ChatTurn::text("user", "hi")], &[weather_tool()], &ToolChoice::Required).unwrap();
        assert!(required.turns[0].content.ends_with("</tool_call>\n\nYou must call a function."));
        let named =
            render_tools_into_turns(&[ChatTurn::text("user", "hi")], &[weather_tool()], &ToolChoice::Named("get_weather".into()))
                .unwrap();
        assert!(named.turns[0].content.ends_with("</tool_call>\n\nYou must call the function `get_weather`."));
        assert_eq!(ToolChoice::Auto.advisory_sentence(), None);
        assert_eq!(ToolChoice::None.advisory_sentence(), None);
        assert_eq!(
            ToolChoice::Named("f".into()).requested_json(),
            serde_json::json!({ "type": "function", "function": { "name": "f" } })
        );
        assert_eq!(ToolChoice::Required.requested_json(), serde_json::json!("required"));
    }

    /// The round-trip's two turns render as the template renders them: an assistant turn's calls
    /// as `<tool_call>` blocks after its content (the leading newline is the renderer's), and a
    /// run of `tool` turns as ONE user turn of `<tool_response>` blocks.
    #[test]
    fn tool_calls_and_tool_responses_render_as_the_templates_text() {
        let history = vec![
            ChatTurn::text("user", "weather in Paris and Rome?"),
            ChatTurn {
                role: "assistant".into(),
                content: String::new(),
                tool_calls: vec![
                    ToolCallTurn { name: "get_weather".into(), arguments: serde_json::json!({ "city": "Paris" }) },
                    ToolCallTurn { name: "get_weather".into(), arguments: serde_json::json!({ "city": "Rome" }) },
                ],
            },
            ChatTurn::text("tool", "{\"temp_c\": 21}"),
            ChatTurn::text("tool", "{\"temp_c\": 27}"),
            ChatTurn::text("user", "and Berlin?"),
        ];
        let rendered = render_tools_into_turns(&history, &[], &ToolChoice::Auto).unwrap();
        assert!(rendered.spoke_tool_convention, "a tool turn is the convention even without a tools list");
        assert_eq!(rendered.turns.len(), 4, "two tool turns merged into one user turn");
        assert_eq!(
            rendered.turns[1],
            Turn {
                role: "assistant".into(),
                content: "<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\":\"Paris\"}}\n</tool_call>\n\
                          <tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\":\"Rome\"}}\n</tool_call>"
                    .into()
            }
        );
        assert_eq!(
            rendered.turns[2],
            Turn {
                role: "user".into(),
                content: "<tool_response>\n{\"temp_c\": 21}\n</tool_response>\n<tool_response>\n{\"temp_c\": 27}\n</tool_response>"
                    .into()
            }
        );
        assert_eq!(rendered.turns[3], Turn { role: "user".into(), content: "and Berlin?".into() });

        // With content, the template writes the content first and the block on the next line —
        // and through `build_prompt` the whole thing is exactly Qwen's `assistant\n` + tail.
        let with_text = vec![ChatTurn {
            role: "assistant".into(),
            content: "Let me check.".into(),
            tool_calls: vec![ToolCallTurn { name: "get_time".into(), arguments: serde_json::json!({}) }],
        }];
        let rendered = render_tools_into_turns(&with_text, &[], &ToolChoice::Auto).unwrap();
        assert_eq!(rendered.turns[0].content, "Let me check.\n<tool_call>\n{\"name\": \"get_time\", \"arguments\": {}}\n</tool_call>");
        let plan =
            build_prompt(&manifest(true), &[rendered.turns[0].clone(), Turn { role: "user".into(), content: "u".into() }]).unwrap();
        assert!(plan.displayed.starts_with(
            "<|im_start|>assistant\nLet me check.\n<tool_call>\n{\"name\": \"get_time\", \"arguments\": {}}\n</tool_call><|im_end|>\n"
        ));
    }

    /// **The tools id is per request, the base id is the model's** (ADR-0096 Decision 2): the
    /// same manifest yields the base id for a plain request and the `-tools` id for one that
    /// spoke the convention, the SEGMENTS differ only by text, and SA-3's declared specials are
    /// identical — the tool block is a `Text` segment like any other user text.
    #[test]
    fn a_request_that_spoke_the_convention_carries_the_tools_id_and_the_same_specials() {
        use misaka_palw_base0::chat_template::{
            TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_TOOLS_V1, TEMPLATE_ID_CHAT_SEGMENTS_TOOLS_V1, TEMPLATE_ID_PLAIN_SEGMENTS_TOOLS_V1,
        };
        let plain_turns =
            render_tools_into_turns(&[ChatTurn::text("system", "s"), ChatTurn::text("user", "u")], &[], &ToolChoice::Auto).unwrap();
        let tool_turns = render_tools_into_turns(
            &[ChatTurn::text("system", "s"), ChatTurn::text("user", "u")],
            &[weather_tool()],
            &ToolChoice::Auto,
        )
        .unwrap();
        for (m, base, tools) in [
            (manifest(true), TEMPLATE_ID_CHAT_SEGMENTS_V1, TEMPLATE_ID_CHAT_SEGMENTS_TOOLS_V1),
            (think_manifest(), TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_V1, TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_TOOLS_V1),
            (manifest(false), TEMPLATE_ID_PLAIN_SEGMENTS_V1, TEMPLATE_ID_PLAIN_SEGMENTS_TOOLS_V1),
        ] {
            let plain = build_prompt_with_tools(&m, &plain_turns).unwrap();
            let with_tools = build_prompt_with_tools(&m, &tool_turns).unwrap();
            assert_eq!(plain.template_id, base, "{}", m.model_id);
            assert_eq!(plain.template_id, template_id_for(&m), "/health advertises the model's transform");
            assert_eq!(with_tools.template_id, tools, "{}", m.model_id);
            assert_ne!(template_id_for(&m), with_tools.template_id, "the tools id is a fact about ONE request, never advertised");
            assert_eq!(plain.declared_specials, with_tools.declared_specials, "the tool block placed no control token");
            assert_eq!(plain.segments.len(), with_tools.segments.len());
            assert!(with_tools.displayed.contains("<tools>\n{\"function\""), "the block is in the text");
            for segment in &with_tools.segments {
                if let PalwFpPromptSegmentV1::Text(bytes) = segment {
                    assert!(!String::from_utf8_lossy(bytes).contains("<|im_"), "no control token rides inside the block");
                }
            }
            // And SA-3 holds over the tools prompt with the same honest ids it holds over the plain one.
            let control = control_token_ids(&m);
            let honest: Vec<u32> = plain.declared_specials.iter().flat_map(|id| [*id, 10]).collect();
            if !honest.is_empty() {
                check_committed_prompt_ids(&with_tools, &honest, &control).expect("the same declared specials");
            }
        }
    }

    /// **The answer side parses; the commitment does not move** (ADR-0096 invariant 8). Two
    /// well-formed blocks become calls, a malformed one and an unterminated one stay in the text
    /// and are counted, and the stream's ids and bytes — what W5 compares and what the roots are
    /// computed over — are untouched by any of it.
    #[test]
    fn tool_call_blocks_parse_out_of_the_shown_answer_and_nothing_committed_moves() {
        let eog: BTreeSet<u32> = [151_645u32].into_iter().collect();
        let mut stream = AnswerStream::new();
        let pieces: [(u32, &str); 6] = [
            (1, "Checking both.\n<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Paris\"}}\n</tool_call>"),
            (2, "\n<tool_call>\n{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Rome\", \"unit\": 1.0}}\n</tool_call>"),
            (3, "\n<tool_call>\nnot json\n</tool_call>"),
            (4, "\n<tool_call>\n{\"name\": \"x\""),
            (151_645, "<|im_end|>"),
            (5, " tail past the display cut"),
        ];
        for (id, text) in pieces {
            stream.push(id, text.as_bytes(), &eog);
        }
        stream.finish();
        let rendered: String = pieces.iter().map(|(_, t)| *t).collect();
        let result = result_with(pieces.iter().map(|(id, _)| *id).collect(), &rendered);
        assert!(check_streamed_answer(&stream, &result).unwrap(), "the honest run binds");
        let roots_before = (result.output_root, result.trace_root, result.output_token_ids.clone(), stream.bytes().to_vec());

        let parsed = parse_tool_calls(&stream.shown());
        assert_eq!(parsed.calls.len(), 2);
        assert_eq!(parsed.calls[0], ParsedToolCall { name: "get_weather".into(), arguments: serde_json::json!({ "city": "Paris" }) });
        assert_eq!(parsed.calls[1].arguments, serde_json::json!({ "city": "Rome", "unit": 1.0 }));
        assert_eq!(parsed.unparsed_blocks, 2, "the malformed block and the unterminated one");
        // The parsed blocks are gone; the text around them — including the newline that preceded
        // each — and the two unparsed blocks are kept verbatim, then the whole is trimmed.
        assert_eq!(parsed.text, "Checking both.\n\n\n<tool_call>\nnot json\n</tool_call>\n<tool_call>\n{\"name\": \"x\"");

        // `parse_tool_calls` takes a `&str` and returns new strings: by construction it reached
        // neither the ids nor the bytes. Pinned anyway, because "by construction" rots.
        assert_eq!((result.output_root, result.trace_root, result.output_token_ids.clone(), stream.bytes().to_vec()), roots_before);
        assert!(check_streamed_answer(&stream, &result).unwrap(), "and W5 still binds the same run");

        // Edge shapes: no block is the identity (trimmed); a block whose object lacks `arguments`
        // is not a call; only-a-call leaves empty text.
        let none = parse_tool_calls("  just text  ");
        assert_eq!(none, ParsedAnswer { text: "just text".into(), calls: vec![], unparsed_blocks: 0 });
        let no_args = parse_tool_calls("<tool_call>{\"name\": \"f\"}</tool_call>");
        assert_eq!((no_args.calls.len(), no_args.unparsed_blocks), (0, 1));
        let only = parse_tool_calls("<tool_call>\n{\"arguments\": {}, \"name\": \"f\"}\n</tool_call>\n");
        assert_eq!(only.text, "");
        assert_eq!(only.calls[0].name, "f");
    }

    /// Call ids are a keyed hash of `(job id, index)`: stable across reads, distinct across
    /// indices and jobs, and OpenAI-shaped.
    #[test]
    fn tool_call_ids_are_a_function_of_the_job_and_the_index() {
        let job = Hash64::from_u64_word(42);
        let id0 = tool_call_id(job, 0);
        assert_eq!(id0, tool_call_id(job, 0));
        assert_eq!(id0.len(), "call_".len() + 24);
        assert!(id0.starts_with("call_") && id0[5..].bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(id0, tool_call_id(job, 1));
        assert_ne!(id0, tool_call_id(Hash64::from_u64_word(43), 0));
    }

    /// `append_to_system_turn` reads `messages[0]` only, as the template does: a later system turn
    /// is an ordinary turn and a created one carries the shared default line.
    #[test]
    fn the_system_turn_is_the_first_turn_or_a_created_one() {
        let mut first = turns(&[("system", "A"), ("user", "u")]);
        append_to_system_turn(&mut first, "\n\nB");
        assert_eq!(first, turns(&[("system", "A\n\nB"), ("user", "u")]));
        let mut later = turns(&[("user", "u"), ("system", "late")]);
        append_to_system_turn(&mut later, "\n\nB");
        assert_eq!(later, turns(&[("system", "You are a helpful assistant.\n\nB"), ("user", "u"), ("system", "late")]));
        assert_eq!(DEFAULT_SYSTEM_TURN, "You are a helpful assistant.");
    }

    /// The persistent-stream frame reader: a clean end is `None`, a truncated frame is an error.
    #[test]
    fn the_frame_reader_tells_a_clean_end_from_a_truncated_one() {
        let mut body = 5u32.to_le_bytes().to_vec();
        body.extend_from_slice(b"hello");
        assert_eq!(read_frame_stream(&mut body.as_slice(), 1 << 20).unwrap().unwrap(), b"hello");
        assert!(read_frame_stream(&mut [].as_slice(), 1 << 20).unwrap().is_none(), "a clean end of stream is not an error");
        let truncated = 5u32.to_le_bytes().to_vec();
        assert!(read_frame_stream(&mut truncated.as_slice(), 1 << 20).is_err(), "a frame that never arrives is an error");
        let too_big = u32::MAX.to_le_bytes().to_vec();
        assert!(read_frame_stream(&mut too_big.as_slice(), 1 << 20).is_err(), "the ceiling is enforced before the allocation");
    }
}
