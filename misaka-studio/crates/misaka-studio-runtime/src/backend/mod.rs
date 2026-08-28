//! **The backend seam.**
//!
//! `Studio UI → MISAKA Runtime API → backend → GPU/CPU`. Everything above this module speaks
//! [`GenerationRequest`] and [`StreamEvent`]; everything below it is llama.cpp, or MLX, or one
//! day the deterministic runtime this repository already carries for PALW. Nothing above names
//! an engine, which is the property that makes the engine replaceable.
//!
//! # Why a trait and not an enum
//!
//! An enum would be shorter today and wrong later: adding the MISAKA runtime should not mean
//! editing every match in the codebase. The trait is dyn-compatible through boxed futures
//! ([`BoxFuture`]) rather than `async fn`, which keeps `Arc<dyn InferenceBackend>` usable as the
//! one handle the API layer holds.
//!
//! # What a backend must answer for
//!
//! Not just tokens — **identity**. [`InferenceBackend::descriptor`] returns the
//! [`RuntimeDescriptor`] that becomes `h_R`, and a backend that cannot say which commit and
//! build profile it is must say so with the literal `unknown` rather than inventing a plausible
//! string. An `h_R` derived from a guess is worse than no `h_R`: it is a number that will not
//! match the machine it claims to describe, discovered only when a verification layer starts
//! comparing them.

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use misaka_studio_core::provenance::{RuntimeDescriptor, SamplingCommitment};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

pub mod llamacpp;
pub mod mlx;
pub mod mock;
pub mod openai_child;

/// One turn in a conversation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        ChatMessage { role: role.into(), content: content.into() }
    }
}

/// What to generate, and how.
#[derive(Clone, Debug)]
pub struct GenerationRequest {
    /// Model id as the Studio knows it — the loaded model, checked by the caller.
    pub model: String,
    /// Chat turns. Empty for a raw-completion request.
    pub messages: Vec<ChatMessage>,
    /// Raw prompt, for `/v1/completions`. Mutually exclusive with `messages`.
    pub prompt: Option<String>,
    pub params: SamplingCommitment,
    pub stop: Vec<String>,
}

/// Token accounting, in OpenAI's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// One event from a generation.
///
/// `Done` carries the usage because that is the only point at which it is known — and because a
/// caller that needs tokens/sec must not have to count tokens itself with a different tokenizer
/// than the one that produced them.
#[derive(Clone, Debug, PartialEq)]
pub enum StreamEvent {
    /// A chunk of generated text.
    Delta(String),
    /// Generation finished normally.
    Done { usage: Usage, finish_reason: String },
}

/// What to load.
#[derive(Clone, Debug)]
pub struct LoadRequest {
    pub model_id: String,
    pub model_path: PathBuf,
    pub context_size: u32,
    /// Layers to place on the accelerator. `None` lets the backend decide.
    pub gpu_layers: Option<u32>,
    pub threads: Option<u32>,
    pub flash_attention: bool,
    pub use_mmap: bool,
    pub use_mlock: bool,
    pub extra_args: Vec<String>,
}

/// What a backend reports about the model it currently holds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadedModel {
    pub model_id: String,
    pub context_size: u32,
    /// Layers actually on the accelerator, when the engine reports it.
    pub gpu_layers: Option<u32>,
    /// How long the load took. The number people want when deciding whether to keep a model
    /// resident.
    pub load_ms: u64,
}

/// An engine, behind one interface.
pub trait InferenceBackend: Send + Sync {
    /// Stable name: `llamacpp`, `mlx`, `mock`.
    fn name(&self) -> &'static str;

    /// The identity of this engine — what `h_R` is derived from.
    ///
    /// Returned as a future because discovering it usually means running the engine's
    /// `--version` and reading what comes back.
    fn descriptor(&self) -> BoxFuture<'_, RuntimeDescriptor>;

    /// Whether this backend can run on this machine at all. A missing binary is a normal answer,
    /// not an error: the UI lists backends and greys out the ones that are not installed.
    fn availability(&self) -> BoxFuture<'_, Availability>;

    fn load(&self, request: LoadRequest) -> BoxFuture<'_, crate::Result<LoadedModel>>;

    fn unload(&self) -> BoxFuture<'_, crate::Result<()>>;

    /// The currently loaded model, if any.
    fn loaded(&self) -> BoxFuture<'_, Option<LoadedModel>>;

    /// Generate, as a stream of events.
    ///
    /// The stream is `'static` so the HTTP layer can hand it straight to a response body without
    /// borrowing the backend for the life of the request.
    fn generate(&self, request: GenerationRequest) -> BoxFuture<'_, crate::Result<BoxStream<'static, crate::Result<StreamEvent>>>>;
}

/// Whether a backend can be used here, and if not, what would fix it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Availability {
    Available { detail: String },
    /// Present in principle, missing something concrete. `remedy` is shown to the user, so it is
    /// an instruction ("install llama.cpp, or set backend.llama_server_path") rather than a
    /// diagnosis.
    Unavailable { reason: String, remedy: String },
}

impl Availability {
    pub fn is_available(&self) -> bool {
        matches!(self, Availability::Available { .. })
    }
}

/// A backend handle, shared.
pub type SharedBackend = Arc<dyn InferenceBackend>;

/// Render chat turns into a single prompt.
///
/// A fallback, used only by backends that have no chat endpoint of their own. The real chat
/// template lives in the GGUF and llama.cpp applies it; a house format applied on top of a model
/// trained on a different one is the classic cause of "the model repeats itself" and
/// "it never stops generating".
pub fn render_fallback_prompt(messages: &[ChatMessage]) -> String {
    let mut out = String::new();
    for m in messages {
        out.push_str(&format!("<|{}|>\n{}\n", m.role, m.content));
    }
    out.push_str("<|assistant|>\n");
    out
}

/// A rough token count for text, used only where the engine does not report usage.
///
/// Deliberately crude — ~4 characters per token — and never used to bill anything or to size a
/// context window. Its one job is keeping a tokens/sec readout from being blank.
pub fn approximate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_prompt_ends_with_the_assistant_turn() {
        let p = render_fallback_prompt(&[ChatMessage::new("system", "be brief"), ChatMessage::new("user", "hi")]);
        assert!(p.ends_with("<|assistant|>\n"), "got {p:?}");
        assert!(p.contains("be brief"));
    }

    #[test]
    fn token_estimates_are_never_zero_for_non_empty_text() {
        assert_eq!(approximate_tokens(""), 0);
        assert_eq!(approximate_tokens("ab"), 1);
        assert_eq!(approximate_tokens("12345678"), 2);
    }
}
