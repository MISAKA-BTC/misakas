//! One module per kind (ADR-0108 Decision 1's list). Each recomputes its kind's identity from what
//! the manifest carries, re-runs the vectors it declares, and — at Full — reads the bytes the
//! person holds; each answers with one of the four tiers.

pub mod certification;
pub mod derived_transformer;
pub mod model_class;
pub mod ruleset_candidate;
