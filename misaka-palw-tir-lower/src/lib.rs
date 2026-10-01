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
//! # The generic frontend (ModelSpec V1)
//!
//! `model_type` selects no code. [`hf_schema::read_model`] turns a configuration (and, optionally, the
//! checkpoint's tensor names and shapes — a safetensors header is enough) into a feature-based
//! [`model::ModelSpec`]:
//!
//! * **Level A** — the standard keys and tensor names suffice (no adapter);
//! * **Level B** — an *adapter*, a data file in the `misaka.palw.model-adapter.v1` format
//!   ([`adapter`], `adapters/*.json`, documented in `docs/design/palw/tir/model-adapter-v1.md`), maps
//!   the class's own keys onto features; no Rust code, no protocol change;
//! * **Level C** — a feature is missing: named ([`model::REGISTRY`]), with the smallest general
//!   primitive that would close a protocol gap.
//!
//! [`model::analyze`] wraps that in the report `palw-class check-architecture` prints
//! ([`model::ArchitectureReport`]: features `SUPPORTED`/`MISSING`, the level, the adapter used, whether
//! a new primitive or court kernel would be needed). [`model::ModelSpec::features`] lists what a spec
//! uses. The per-architecture parsers of [`hf_config`] remain only as the oracle of
//! `tests/adapters.rs` and as the route of families not yet converted into adapter files.
//!
//! Nothing here depends on consensus crates; nothing here is on a validation path.

pub mod adapter;
pub mod admission;
pub mod artifact;
pub mod calib;
pub mod cfg;
pub mod convert;
pub mod detmath;
pub mod encoder;
pub mod error;
pub mod fidelity;
pub mod float_ref;
pub mod gguf;
pub mod hf_config;
pub mod hf_schema;
pub mod hf_weights;
pub mod hl;
pub mod lora;
pub mod lower;
pub mod model;
pub mod ngram;
pub mod prequant;
pub mod quantfmt;
pub mod quant;
pub mod report;
pub mod rope;
pub mod spec;
pub mod weights;

pub use error::{LowerError, Result};
pub use hf_config::{parse_config, parse_config_str, parse_config_str_with, parse_config_with};
