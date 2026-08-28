//! **MISAKA Studio core** — the vocabulary the UI, the Runtime and (eventually) the chain all
//! read a model through.
//!
//! Nothing here talks to a network, spawns a process, or serves a request. It answers the four
//! questions a local-LLM app has to answer before it can do anything useful:
//!
//! * **What is this file?** [`gguf`] reads a GGUF's own header — architecture, layer count,
//!   context length, tensor shapes — instead of guessing from the filename.
//! * **How was it quantized?** [`quant`] names the scheme, its bits-per-weight, and what that
//!   trade costs, from the GGUF's `general.file_type` where it exists and the filename where it
//!   does not.
//! * **Will it run here?** [`hardware`] probes the machine and [`model`] turns
//!   (file size, layers, context) into a working-set estimate and a verdict.
//! * **Which exact artifact produced this answer?** [`provenance`] derives the model and runtime
//!   identities **byte-for-byte the way consensus derives them**, so a Studio inference can one
//!   day be handed to the MISAKA network without re-deriving anything.
//!
//! # The provenance seam
//!
//! The roadmap is `Inference → Deterministic Execution → Inference Hash → Verification →
//! Compute Credit → PALW → MISAKA Network`, and the initial version implements exactly the first
//! and third of those. That is on purpose: what the later stages need from an app is not a
//! blockchain client, it is *the identity of what ran*. If the Studio records that faithfully
//! from day one, the rest is additive; if it does not, no later version can reconstruct it.
//!
//! So [`provenance::derive_model_weights_hash`] is not a Studio-flavoured hash — it is the same
//! keyed BLAKE2b-512 derivation as `kaspa_consensus_core::vlt::derive_model_weights_hash`, with
//! the same domain key and the same field order, checked against golden vectors in
//! `provenance::tests`. A model this app downloads therefore has the same `h_M` the chain would
//! give it.

pub mod gguf;
pub mod hardware;
pub mod model;
pub mod provenance;
pub mod quant;
pub mod settings;

pub use gguf::{GgufMetadata, GgufValue};
pub use hardware::{Accelerator, HardwareSnapshot};
pub use model::{FitVerdict, LocalModel, ModelRequirements};
pub use provenance::{InferenceRecord, ModelIdentity, RuntimeIdentity};
pub use quant::Quantization;

/// Everything in this crate that can fail, in one enum.
///
/// A desktop app shows these to a person who did not write them, so each variant carries the
/// thing that went wrong — the path, the offset, the field — rather than a category.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: String, source: std::io::Error },

    #[error("{path} is not a GGUF file: expected the magic 'GGUF', found {found:02x?}")]
    NotGguf { path: String, found: [u8; 4] },

    #[error(
        "{path} is GGUF version {version}; MISAKA Studio reads versions 2 and 3. Version 1 used \
         32-bit lengths and no shipped model still uses it — re-export the model with a current \
         llama.cpp conversion script."
    )]
    UnsupportedGgufVersion { path: String, version: u32 },

    #[error("{path}: malformed GGUF at byte {offset}: {reason}")]
    MalformedGguf { path: String, offset: u64, reason: String },

    #[error("{path}: {reason}")]
    Settings { path: String, reason: String },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn io(path: impl std::fmt::Display, source: std::io::Error) -> Self {
        Error::Io { path: path.to_string(), source }
    }
}
