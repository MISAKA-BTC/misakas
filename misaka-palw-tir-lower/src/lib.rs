//! # `misaka-palw-tir-lower` — the Hugging Face side of RFC-0002 (PALW-TIR)
//!
//! Non-consensus tooling. A Hugging Face decoder checkpoint (`config.json` + safetensors) is
//! described exactly, outside consensus, so that it can later be expanded into PALW-TIR through
//! the composite-op library (Gate 2):
//!
//! ```text
//!  config.json ──hf_config──▶ ArchSpec ─┬─ math ──hl::build──▶ HlProgram (frontend-neutral)
//!                                       └─ HfStorage ──hf_weights──▶ Binding (param → Src)
//!  safetensors ──weights──▶ tensors ──Binding──▶ HL params ──float_ref──▶ logits, site stats
//! ```
//!
//! * [`hf_config`] parses `config.json` into a normalised [`spec::ArchSpec`], refusing
//!   (`NOT_LOWERABLE`) every key it does not understand and every value it does not implement.
//! * [`hl`] builds the high-level graph organised as `TirProgramV1` will be: named blocks, a
//!   per-layer schedule of block kinds, per-layer params, `Fixed`/`Hist` states, one position per
//!   step, and a named requantisation **site** at every op boundary. It carries only math-level
//!   attributes — no HF key, tensor name or fused layout — so an ONNX or GGUF importer can
//!   produce the identical graph.
//! * [`hf_weights`] is the HF weight-name mapping: every HL param ← an HF tensor expression.
//! * [`weights`] reads safetensors (single and sharded; bf16/f16/f32) and evaluates the
//!   expressions with shape checks.
//! * [`float_ref`] interprets the HL graph in f32 (f64 accumulation), faithful to `transformers`.
//!
//! Nothing here depends on consensus crates; nothing here is on a validation path.

pub mod cfg;
pub mod error;
pub mod float_ref;
pub mod hf_config;
pub mod hf_weights;
pub mod hl;
pub mod report;
pub mod rope;
pub mod spec;
pub mod weights;

pub use error::{LowerError, Result};
pub use hf_config::{parse_config, parse_config_str};
