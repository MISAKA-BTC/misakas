//! # `misaka-palw-tir-lower` — the Hugging Face side of RFC-0002 (PALW-TIR)
//!
//! Non-consensus tooling. A Hugging Face decoder checkpoint (`config.json` + safetensors) is
//! described exactly, outside consensus, and expanded into a PALW-TIR program plus its integer
//! params (Gate 2a):
//!
//! ```text
//!  config.json ──hf_config──▶ ArchSpec ─┬─ math ──hl::build──▶ HlProgram (frontend-neutral)
//!                                       └─ HfStorage ──hf_weights──▶ Binding (param → Src)
//!  safetensors ──weights──▶ tensors ──Binding──▶ HL params ──float_ref──▶ logits, site stats
//!  HlProgram ──lower──▶ TirProgramV1 + fills ──(float params, site stats)──▶ integer artifact
//!  TirProgramV1 + artifact ──misaka-palw-tir evaluator──▶ integer logits ──fidelity──▶ top-1, KL, ppl
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
//! * [`float_ref`] interprets the HL graph in f32 (f64 accumulation), faithful to `transformers`;
//!   [`float_ref::stream`] runs it layer by layer so a real checkpoint streams.
//! * [`lower`] expands the HL graph into a `TirProgramV1` (A16 activations, W8 weights, Q24
//!   internals) and fills its params from a checkpoint and calibration statistics ([`quant`]).
//! * [`artifact`] writes the params (crate-local format with a digest); [`fidelity`] runs the
//!   integer program on the reference evaluator against the float reference.
//!
//! Nothing here depends on consensus crates; nothing here is on a validation path.

pub mod artifact;
pub mod cfg;
pub mod error;
pub mod fidelity;
pub mod float_ref;
pub mod hf_config;
pub mod hf_weights;
pub mod hl;
pub mod lower;
pub mod quant;
pub mod report;
pub mod rope;
pub mod spec;
pub mod weights;

pub use error::{LowerError, Result};
pub use hf_config::{parse_config, parse_config_str};
