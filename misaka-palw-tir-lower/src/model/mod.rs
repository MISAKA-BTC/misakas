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
pub mod scope;

pub use crate::spec::ModelSpec;
pub use features::{
    Area, BASE_PRIMITIVES, FeatureId, FeatureInfo, FeatureUse, Lowering, REGISTRY, Requirement, encdec_features, feature_info,
};
pub use crate::hf_schema::{AdapterSource, Level, MissingItem};
pub use report::{ArchitectureReport, FeatureReport, FeatureStatus, ReportResult, analyze, analyze_headers_with, analyze_with};
pub use scope::{Excluded, FeatureScope, SCOPE_SCHEMA_V1, scope_of, sibling_files};

/// The schema id of a serialised [`ModelSpec`] (what an adapter file's `spec` template instantiates).
pub const MODEL_SPEC_SCHEMA_V1: &str = "misaka.palw.model-spec.v1";

/// The domain key of a [`ModelSpec`]'s digest.
pub const MODEL_SPEC_DIGEST_KEY_V1: &[u8] = b"misaka-palw/model-spec/v1";

/// **The identity of a spec**: BLAKE2b-256, keyed, over the canonical JSON (keys sorted, compact,
/// integral floats as integers — the adapter files' form) of the serialised [`ModelSpec`]. Two models
/// with equal digests compute the same function whatever they are called; a runtime pack records it
/// beside the adapter's hash, so a verifier sees that the same adapter read the same configuration to
/// the same description.
pub fn spec_digest(spec: &ModelSpec) -> String {
    let v = serde_json::to_value(spec).unwrap_or(serde_json::Value::Null);
    let h = blake2b_simd::Params::new().hash_length(32).key(MODEL_SPEC_DIGEST_KEY_V1).hash(crate::adapter::canonical_json(&v).as_bytes());
    h.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// A digest of the feature vocabulary this build knows (ids, areas, lowering status, protocol
/// requirement): what a pack pins so that "feature X is supported" means the same thing to its verifier.
pub fn registry_digest() -> String {
    let mut st = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/feature-registry/v1").to_state();
    for f in REGISTRY {
        st.update(f.id.0.as_bytes());
        st.update(&[0]);
        st.update(format!("{:?}|{:?}|{:?}", f.area, f.lowering, f.protocol).as_bytes());
        st.update(b"\n");
    }
    st.finalize().as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
