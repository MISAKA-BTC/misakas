//! # `ModelSpec` V1 — the feature-based description every frontend produces
//!
//! ```text
//!  HF config / weights ──hf_schema──▶ ModelSpec ──features()──▶ FeatureId set
//!                                        │
//!                                        └─ hl::build ──▶ HL ──lower──▶ PALW-TIR ──▶ admission, executor, court
//! ```
//!
//! [`ModelSpec`] is the normalised, frontend-neutral description of a model (the type formerly
//! called `ArchSpec`; the name remains as an alias). `model_type` is informational: two models with
//! equal specs compute the same function whatever they are called, and a model nobody has heard of is
//! a new *combination* of [`features`], never a new primitive, runtime or court kernel.
//!
//! * [`features`] — the finite, versioned feature vocabulary ([`FeatureId`], [`REGISTRY`]) and
//!   [`ModelSpec::features`].
//! * [`report`] — what `palw-class check-architecture` prints: the features a model needs, each
//!   `SUPPORTED` or `MISSING` (with the capability that would close the gap), the support
//!   [`Level`], and whether the protocol would need anything new.
//!
//! The reader that produces a spec from a Hugging Face directory is [`crate::hf_schema`].

pub mod features;
pub mod report;

pub use crate::spec::ModelSpec;
pub use features::{
    Area, BASE_PRIMITIVES, FeatureId, FeatureInfo, FeatureUse, Lowering, REGISTRY, Requirement, encdec_features, feature_info,
};
pub use crate::hf_schema::{AdapterSource, Level, MissingItem};
pub use report::{ArchitectureReport, FeatureReport, FeatureStatus, ReportResult, analyze};

/// The schema id of a serialised [`ModelSpec`] (what an adapter file's `spec` template instantiates).
pub const MODEL_SPEC_SCHEMA_V1: &str = "misaka.palw.model-spec.v1";
