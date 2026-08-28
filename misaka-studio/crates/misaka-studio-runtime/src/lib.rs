//! **MISAKA Runtime** — the layer between the Studio's UI and whatever is doing the arithmetic.
//!
//! ```text
//!   Studio UI  (React, or any OpenAI client)
//!       │  HTTP: /v1/… and /api/v1/…
//!   MISAKA Runtime API           ← this crate
//!       │  InferenceBackend
//!   LLM backend                  ← llama.cpp today, MLX on Apple, MISAKA's own runtime later
//!       │
//!   GPU / CPU
//! ```
//!
//! The seam that matters is the second one. Everything above it speaks
//! [`GenerationRequest`](backend::GenerationRequest) and
//! [`StreamEvent`](backend::StreamEvent); no route, no UI component and no record names an
//! engine. Replacing llama.cpp with the deterministic runtime this repository already carries
//! for PALW is then an implementation of one trait, not a rewrite.
//!
//! The other thing this layer does that a thin proxy would not: it records **what ran**. Every
//! completion produces an [`InferenceRecord`](misaka_studio_core::provenance::InferenceRecord)
//! binding the model identity, the runtime identity and commitments to the prompt and the
//! output — the same derivations consensus uses, so the log is already in the shape a
//! verification layer would consume.

pub mod api;
pub mod backend;
pub mod catalog;
pub mod download;
pub mod error;
pub mod metrics;
pub mod records;
pub mod state;
pub mod store;

pub use error::{Error, Result};
pub use state::AppState;

/// Where the UI bundle is, if one is available.
///
/// Checked in the order that gets each kind of user the right answer: an explicit flag for
/// someone who knows, then the packaged layout beside the executable, then the development tree.
pub fn locate_ui(explicit: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    if let Some(dir) = explicit {
        return dir.is_dir().then_some(dir);
    }
    if let Ok(var) = std::env::var("MISAKA_STUDIO_UI_DIR") {
        let dir = std::path::PathBuf::from(var);
        if dir.is_dir() {
            return Some(dir);
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        for candidate in [parent.join("ui"), parent.join("../ui"), parent.join("../share/misaka-studio/ui")] {
            if candidate.join("index.html").is_file() {
                return Some(candidate);
            }
        }
    }
    let dev = std::path::PathBuf::from("ui/dist");
    dev.join("index.html").is_file().then_some(dev)
}
