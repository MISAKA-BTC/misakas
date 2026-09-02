//! # misaka-palw-derive — ADR-0078: what was made from it is committed; the thing never rides
//!
//! The layer above ADR-0077's receipt. A certified free-prompt claim commits the model's output
//! ids (`output_root`). This crate turns that answer into a thing a person keeps — a GLB, a PNG,
//! a MIDI file, a map, a trace — through two pure functions and names the result on the chain
//! with one compact object:
//!
//! ```text
//! answer bytes ──grammar.canonicalize──▶ canonical DSL ──transformer.run──▶ artifact
//!                    (grammar_id, dsl_hash)                (transformer_id, artifact_hash, artifact_bytes)
//! ```
//!
//! What this crate is NOT: consensus. The object type, its id and the transition that accepts
//! it live in `kaspa-consensus-core` (`palw_derived_v1`), and the chain never runs a transformer
//! (ADR-0078 Decision 5). This crate is the executor's and the consumer's: the gateway derives
//! here, and anyone holding the answer verifies here.
//!
//! Discipline (Decision 3): every transformer is pure Rust in the tree, integer or exact
//! arithmetic, no clock, no randomness, no network, and its manifest carries the build's
//! source-tree hash (`build.rs`), so `transformer_id` names the code. The drill
//! (`palw-derive drill`) runs the corpus on two architectures and requires byte-identical
//! artifacts before a transformer may be named by any object.

pub mod bytes;
pub mod canon_json;
pub mod checksum;
pub mod derive;
pub mod fixed;
pub mod ids;
pub mod kinds;
pub mod registry;
pub mod zlib;

pub use derive::{ClaimBinding, Derivation, Verification, derive_named, derive_with, verify, verify_artifact_bytes};

use thiserror::Error;

/// Every way a derivation can fail. `Grammar` is ADR-0078 X4: the answer did not parse under
/// the grammar, no object is produced, and the claim is untouched. `Inexact` is the discipline's
/// refusal: an output format could not hold a value exactly, and the transformer refuses rather
/// than rounds. `Transformer` is a semantic refusal inside a kind (an unknown primitive, a
/// bound exceeded).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DeriveError {
    #[error("grammar: {0}")]
    Grammar(String),
    #[error("inexact: {0}")]
    Inexact(String),
    #[error("transformer: {0}")]
    Transformer(String),
    #[error("unknown grammar {0}")]
    UnknownGrammar(String),
    #[error("unknown transformer {0}")]
    UnknownTransformer(String),
    #[error("mismatch: {0}")]
    Mismatch(String),
}

/// The build's source-tree hash (SHA-256 over `src/**/*.rs`, sorted; see `build.rs`) — the
/// field a transformer manifest carries so that its id names this code.
pub const SOURCE_TREE_SHA256_HEX: &str = env!("PALW_DERIVE_SOURCE_TREE_SHA256");

/// The arithmetic discipline a transformer declares (ADR-0078 Decision 3). `Integer` is the
/// default for every kind in the table; `ExactRational` is the CAD kernel's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Discipline {
    Integer,
    ExactRational,
}

impl Discipline {
    pub fn as_str(self) -> &'static str {
        match self {
            Discipline::Integer => "integer",
            Discipline::ExactRational => "exact-rational",
        }
    }
}

/// A grammar: the canonicalizer of one kind's DSL. Pure — whitespace, key order, number form,
/// nothing semantic (Decision 2).
pub trait Grammar: Send + Sync {
    /// The grammar's name, e.g. `scene/v1`. Its id is `H(domain ‖ name)` (`ids::grammar_id`).
    fn name(&self) -> &'static str;
    /// Canonicalize an answer, or refuse it (X4).
    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError>;
}

/// The bytes a transformer made, with the format's conventional media type and extension so
/// a gateway can hand them to a browser and a CLI can name the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub bytes: Vec<u8>,
    pub media_type: &'static str,
    pub extension: &'static str,
}

/// A transformer's manifest — the preimage of `transformer_id` (Decision 3): the kind, the
/// grammar it consumes, the build it is, the discipline it declares, and the canonical writer
/// its output uses. Serialized canonically by `ids::transformer_manifest_bytes`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformerManifest {
    /// e.g. `scene/glb/v1`
    pub name: &'static str,
    /// the kind id from the kind table (`kaspa_consensus_core::palw_derived_v1::kind`)
    pub kind: u16,
    /// the grammar name this transformer consumes
    pub grammar: &'static str,
    pub discipline: Discipline,
    /// the canonical writer's name and version, e.g. `gltf-binary/2.0/canonical-v1`
    pub writer: &'static str,
    /// the build's source-tree hash, hex
    pub source_tree_sha256: &'static str,
}

/// A transformer: a pure function from canonical DSL bytes to an artifact (Decision 3).
pub trait Transformer: Send + Sync {
    fn manifest(&self) -> TransformerManifest;
    /// Run on CANONICAL DSL bytes (the grammar's output). A transformer may assume the input is
    /// canonical and must refuse, not repair, anything else.
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError>;
}
