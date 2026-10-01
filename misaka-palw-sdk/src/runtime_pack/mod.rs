//! **Runtime packs** (`misaka.palw.runtime-pack.v1`, RFC-0002 Part II): the manifest that makes a class's
//! artifact reproducible and its claims checkable — `palw-class pack build`, `verify`, `show`.
//!
//! * [`manifest`] — the document, its canonical form and its digest.
//! * [`build`] — convert a model and write the pack.
//! * [`verify`] — check a pack against the source, this build, an artifact and the executors.
//! * [`conformance`] — the reference evaluator, the typed backend and the independent implementation on
//!   the same vectors.
//! * [`hfref`] — the Hugging Face reference an integer program is held to, and the unit check.
//! * [`provenance`] — what an artifact's own provenance record says (scope, adapter, descriptors, math).

pub mod build;
pub mod cli;
pub mod conformance;
pub mod hfref;
pub mod manifest;
pub mod provenance;
pub mod verify;

pub use build::{BuildOpts, BuiltPack, DeclareOpts, build as build_pack};
pub use manifest::{PACK_FILE, PACK_SCHEMA_V1, RuntimePackV1};
pub use verify::{Status, VerifyOpts, VerifyReport, verify as verify_pack};
