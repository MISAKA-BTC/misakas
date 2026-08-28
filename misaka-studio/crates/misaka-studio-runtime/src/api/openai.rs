//! The OpenAI-compatible surface.
//!
//! Compatibility here means a client written for `api.openai.com` works against
//! `http://127.0.0.1:1338/v1` with nothing changed but the base URL — same request fields, same
//! response envelope, same SSE framing down to the terminating `data: [DONE]`.
//!
//! # Just-in-time loading
//!
//! `"model": "Qwen3-4B-Q4_K_M"` on a request for a model that is not loaded loads it. Without
//! this, every client would need a Studio-specific "load first" call, which is exactly the
//! non-compatibility the endpoint exists to avoid. Loading is serialised by the backend and the
//! request waits for it, so the first call after a cold start is slow and correct rather than
//! fast and wrong.
//!
//! # Extra sampling fields
//!
//! `top_k`, `min_p` and `repeat_penalty` are not OpenAI fields; they are what local engines
//! actually expose, and leaving them out would make the Studio's own UI unable to use its own
//! API. They are additive — a client that never sends them gets the configured defaults.

use crate::backend::{ChatMessage, StreamEvent, Usage};
use crate::state::AppState;
use crate::{Error, Result};
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use misaka_studio_core::provenance::SamplingCommitment;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/completions", post(completions))
}

/// `stop` is a string in some clients and a list in others.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum StopField {
    One(String),
    Many(Vec<String>),
}

impl StopField {
    fn into_vec(self) -> Vec<String> {
        match self {
            StopField::One(s) => vec![s],
            StopField::Many(v) => v,
        }
    }
}

/// The sampling fields both endpoints share.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SamplingFields {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<i64>,
    pub min_p: Option<f64>,
    #[serde(alias = "repetition_penalty", alias = "frequency_penalty")]
    pub repeat_penalty: Option<f64>,
    pub max_tokens: Option<u64>,
    pub seed: Option<u64>,
    pub stop: Option<StopField>,
}

impl SamplingFields {
    /// Request values over configured defaults.
    fn resolve(self, defaults: SamplingCommitment) -> (SamplingCommitment, Vec<String>) {
        let params = SamplingCommitment {
            temperature: self.temperature.unwrap_or(defaults.temperature),
            top_p: self.top_p.unwrap_or(defaults.top_p),
            top_k: self.top_k.unwrap_or(defaults.top_k),
            min_p: self.min_p.unwrap_or(defaults.min_p),
            repeat_penalty: self.repeat_penalty.unwrap_or(defaults.repeat_penalty),
            max_tokens: self.max_tokens.unwrap_or(defaults.max_tokens),
            seed: self.seed.or(defaults.seed),
        };
        (params, self.stop.map(StopField::into_vec).unwrap_or_default())
    }
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub sampling: SamplingFields,
}

#[derive(Debug, Deserialize)]
pub struct CompletionRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub prompt: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub sampling: SamplingFields,
}

#[derive(Serialize)]
struct ModelList {
    object: &'static str,
    data: Vec<ModelEntry>,
}

#[derive(Serialize)]
struct ModelEntry {
    id: String,
    object: &'static str,
    created: u64,
    owned_by: &'static str,
}

async fn list_models(State(state): State<Arc<AppState>>) -> Result<Json<ModelList>> {
    let models = state.store.list().await;
    Ok(Json(ModelList {
        object: "list",
        data: models
            .iter()
            .map(|m| ModelEntry { id: m.id.clone(), object: "model", created: m.modified_at.unwrap_or(0), owned_by: "misaka-studio" })
            .collect(),
    }))
}

/// Make sure the requested model is the loaded one.
async fn ensure_loaded(state: &Arc<AppState>, requested: Option<String>) -> Result<String> {
    let current = state.loaded().await.map(|s| s.model.id);
    match (requested, current) {
        (Some(want), Some(have)) if want == have => Ok(have),
        (Some(want), _) => {
            state.load(&want, None).await?;
            Ok(want)
        }
        (None, Some(have)) => Ok(have),
        (None, None) => Err(Error::NoModelLoaded),
    }
}

async fn chat_completions(State(state): State<Arc<AppState>>, Json(request): Json<ChatCompletionRequest>) -> Result<Response> {
    if request.messages.is_empty() {
        return Err(Error::bad_request("messages must not be empty"));
    }
    let model = ensure_loaded(&state, request.model.clone()).await?;
    let defaults = state.settings.read().await.generation.sampling();
    let (params, stop) = request.sampling.resolve(defaults);

    let stream = state.generate(request.messages, None, params, stop).await?;
    Ok(if request.stream { sse_response(stream, model, true) } else { aggregate(stream, model, true).await? })
}

async fn completions(State(state): State<Arc<AppState>>, Json(request): Json<CompletionRequest>) -> Result<Response> {
    let model = ensure_loaded(&state, request.model.clone()).await?;
    let defaults = state.settings.read().await.generation.sampling();
    let (params, stop) = request.sampling.resolve(defaults);

    let stream = state.generate(Vec::new(), Some(request.prompt), params, stop).await?;
    Ok(if request.stream { sse_response(stream, model, false) } else { aggregate(stream, model, false).await? })
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn completion_id(chat: bool) -> String {
    let uuid = uuid::Uuid::new_v4().simple().to_string();
    if chat { format!("chatcmpl-{uuid}") } else { format!("cmpl-{uuid}") }
}

/// Collect the whole generation into one response.
async fn aggregate(mut stream: futures_util::stream::BoxStream<'static, Result<StreamEvent>>, model: String, chat: bool) -> Result<Response> {
    let mut text = String::new();
    let mut usage = Usage::default();
    let mut finish_reason = "stop".to_string();
    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::Delta(delta) => text.push_str(&delta),
            StreamEvent::Done { usage: u, finish_reason: r } => {
                usage = u;
                finish_reason = r;
            }
        }
    }

    let id = completion_id(chat);
    let choice = if chat {
        serde_json::json!({ "index": 0, "message": { "role": "assistant", "content": text }, "finish_reason": finish_reason })
    } else {
        serde_json::json!({ "index": 0, "text": text, "finish_reason": finish_reason })
    };
    Ok(Json(serde_json::json!({
        "id": id,
        "object": if chat { "chat.completion" } else { "text_completion" },
        "created": now(),
        "model": model,
        "choices": [choice],
        "usage": usage,
    }))
    .into_response())
}

/// Stream the generation as server-sent events, in OpenAI's chunk shape.
fn sse_response(stream: futures_util::stream::BoxStream<'static, Result<StreamEvent>>, model: String, chat: bool) -> Response {
    let id = completion_id(chat);
    let created = now();
    let object = if chat { "chat.completion.chunk" } else { "text_completion" };

    // The role-only opening chunk. OpenAI sends one and some clients rely on it to open the
    // assistant message before any text arrives.
    let opener = if chat {
        Some(serde_json::json!({
            "id": id, "object": object, "created": created, "model": model,
            "choices": [{ "index": 0, "delta": { "role": "assistant" }, "finish_reason": null }]
        }))
    } else {
        None
    };

    let events = futures_util::stream::iter(opener.map(|v| Ok(Event::default().data(v.to_string()))))
        .chain(stream.map(move |event| {
            let json = match event {
                Ok(StreamEvent::Delta(delta)) => {
                    let choice = if chat {
                        serde_json::json!({ "index": 0, "delta": { "content": delta }, "finish_reason": null })
                    } else {
                        serde_json::json!({ "index": 0, "text": delta, "finish_reason": null })
                    };
                    serde_json::json!({ "id": id, "object": object, "created": created, "model": model, "choices": [choice] })
                }
                Ok(StreamEvent::Done { usage, finish_reason }) => {
                    let choice = if chat {
                        serde_json::json!({ "index": 0, "delta": {}, "finish_reason": finish_reason })
                    } else {
                        serde_json::json!({ "index": 0, "text": "", "finish_reason": finish_reason })
                    };
                    serde_json::json!({
                        "id": id, "object": object, "created": created, "model": model,
                        "choices": [choice], "usage": usage
                    })
                }
                // An error mid-stream cannot change the status code — the 200 and the headers are
                // long gone. OpenAI's own answer is an error object in the stream, so that is
                // what a client sees here too.
                Err(e) => serde_json::json!({ "error": { "message": e.to_string(), "type": e.openai_type() } }),
            };
            Ok::<Event, Infallible>(Event::default().data(json.to_string()))
        }))
        .chain(futures_util::stream::iter([Ok(Event::default().data("[DONE]"))]));

    Sse::new(events).keep_alive(KeepAlive::default()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_values_override_defaults_and_absent_ones_do_not() {
        let defaults = SamplingCommitment { temperature: 0.7, top_k: 40, max_tokens: 2048, ..Default::default() };
        let fields = SamplingFields { temperature: Some(0.1), stop: Some(StopField::One("</s>".into())), ..Default::default() };
        let (params, stop) = fields.resolve(defaults);
        assert_eq!(params.temperature, 0.1, "the request wins");
        assert_eq!(params.top_k, 40, "an absent field keeps the default");
        assert_eq!(params.max_tokens, 2048);
        assert_eq!(stop, vec!["</s>".to_string()]);
    }

    #[test]
    fn stop_parses_as_either_a_string_or_a_list() {
        let one: SamplingFields = serde_json::from_str("{\"stop\":\"###\"}").expect("string form");
        assert_eq!(one.stop.expect("stop").into_vec(), vec!["###".to_string()]);
        let many: SamplingFields = serde_json::from_str(r#"{"stop":["a","b"]}"#).expect("list form");
        assert_eq!(many.stop.expect("stop").into_vec(), vec!["a".to_string(), "b".to_string()]);
    }

    /// A chat request from a stock OpenAI client must parse, extra local fields and all.
    #[test]
    fn an_openai_shaped_request_parses() {
        let body = r#"{
            "model": "Qwen3-4B-Q4_K_M",
            "messages": [{"role":"system","content":"be brief"},{"role":"user","content":"hi"}],
            "stream": true, "temperature": 0.2, "max_tokens": 128, "top_k": 20, "seed": 7
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(body).expect("parses");
        assert_eq!(request.messages.len(), 2);
        assert!(request.stream);
        assert_eq!(request.sampling.top_k, Some(20));
        assert_eq!(request.sampling.seed, Some(7));
    }

    #[test]
    fn completion_ids_carry_the_expected_prefix() {
        assert!(completion_id(true).starts_with("chatcmpl-"));
        assert!(completion_id(false).starts_with("cmpl-"));
    }
}
