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
/// ADR-0078 Decision 3's "build's source-tree hash": the walk, the framing and the SHA-256 that
/// make `transformer_id` name this code, in the one place `build.rs` and the crate both read it
/// from.
pub mod source_tree;
pub mod zlib;

pub use derive::{
    BoundVerification, ClaimBinding, Derivation, NamedInput, Verification, check_declared_bounds, check_named_inputs,
    check_offered_named_input, check_tokenizer_pin_v1, derive_named, derive_with, named_input_hash_v1, opened_tokenizer_id_v1,
    recompute_output_root, render_answer_v1, rendered_output_hash_for_family, verify, verify_artifact_bytes, verify_bound,
    verify_output_root,
};

use thiserror::Error;

/// Every way a derivation can fail. `Grammar` is ADR-0078 X4: the answer did not parse under
/// the grammar, no object is produced, and the claim is untouched. `Inexact` is the discipline's
/// refusal: an output format could not hold a value exactly, and the transformer refuses rather
/// than rounds. `Transformer` is a semantic refusal inside a kind (an unknown primitive, a
/// bound exceeded). `Bound` is SA-2's: a manifest bound was exceeded, or the transformer
/// publishes none. `UnpublishedManifest` is SA-5's.
///
/// Every variant is a REFUSAL — "no object", Decision 2's parse-failure arm — and
/// [`DeriveError::is_refusal`] says so in one place, so a caller does not have to enumerate the
/// list and quietly mis-file the next variant as an internal error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DeriveError {
    #[error("grammar: {0}")]
    Grammar(String),
    #[error("inexact: {0}")]
    Inexact(String),
    #[error("transformer: {0}")]
    Transformer(String),
    /// ADR-0078 SA-2: a declared input/output/step bound was exceeded before or after the
    /// transformer ran, or the transformer declares none. Exceeding a bound is "no object".
    #[error("bound: {0}")]
    Bound(String),
    /// ADR-0078 SA-5: no transformer manifest is published in this tree at that `transformer_id`,
    /// so no consumer could verify a derivation naming it.
    #[error("unpublished manifest: {0}")]
    UnpublishedManifest(String),
    #[error("unknown grammar {0}")]
    UnknownGrammar(String),
    #[error("unknown transformer {0}")]
    UnknownTransformer(String),
    #[error("mismatch: {0}")]
    Mismatch(String),
}

impl DeriveError {
    /// Whether this is a refusal of the answer rather than a fault of the caller's request.
    /// A refusal is X4's arm: no object exists, the claim is untouched, and a gateway reports it
    /// to the user as a refusal with a reason. `UnknownGrammar` / `UnknownTransformer` /
    /// `Mismatch` are not refusals: they say the REQUEST named something this build does not
    /// have, which is the caller's error and not the answer's.
    pub fn is_refusal(&self) -> bool {
        matches!(
            self,
            DeriveError::Grammar(_)
                | DeriveError::Inexact(_)
                | DeriveError::Transformer(_)
                | DeriveError::Bound(_)
                | DeriveError::UnpublishedManifest(_)
        )
    }
}

/// The build's source-tree hash (SHA-256 over every non-dot file under `src/`, globally sorted
/// by relative path; see [`source_tree`]) — the field a transformer manifest carries so that its
/// id names this code. [`source_tree::source_tree_sha256_hex`] recomputes it from a checkout, so
/// the claim "this id names that code" is one a consumer can check rather than one they must
/// take.
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
/// grammar it consumes, the build it is, the discipline it declares, the canonical writer its
/// output uses, and (SA-2) the bounds it declares. Serialized canonically by
/// `ids::transformer_manifest_bytes`.
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
    /// ADR-0078 SA-2: the bounds this transformer enforces BEFORE it runs — the most DSL bytes it
    /// will look at, the largest artifact it will hand out, and its step ceiling in the kind's own
    /// unit.
    pub max_dsl_bytes: u64,
    pub max_artifact_bytes: u64,
    pub max_steps: u64,
}

impl TransformerManifest {
    /// What [`TransformerManifest::max_steps`] counts — `simulation-step`, `raster-pixel`,
    /// `evm-gas`, … SA-2 asks for "`max_steps`, or the kind's own unit", and a number without its
    /// unit is not a bound. It is a LABEL and not a manifest field: the preimage of
    /// `transformer_id` carries the three numbers, and the unit is what this build calls them, so
    /// it belongs beside the kind table rather than in the hash.
    pub fn step_unit(&self) -> &'static str {
        bounds::step_unit(self.kind)
    }

    /// SA-3's limits on the hash-named inputs of a transformation (Decision 10) — see
    /// [`NamedInputLimits`].
    pub fn named_input_limits(&self) -> NamedInputLimits {
        bounds::named_input_limits(self.kind)
    }
}

/// **SA-3: what a transformer will accept of the bytes a stranger uploads.** Zero is the default
/// and it is fail-closed: a transformer that takes no second input refuses every upload, so a
/// gateway cannot be made to hold bytes for a transformer that would never read them. This is not
/// a manifest field for the same reason the step unit is not: today every kind declares zero, and
/// a transformation kind that admits uploads will state its two numbers where its kind's row is,
/// under the source-tree hash that already names its code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamedInputLimits {
    pub max_inputs: u32,
    pub max_bytes: u64,
}

/// **SA-2's first gate, for a kind that has no wording of its own.** The ceiling is checked on the
/// byte COUNT before a parser is asked what the bytes spell — a JSON parser is an allocator driven
/// by its input, and a bound applied after parsing is a bound applied after the damage. Exceeding
/// it is "no object" through Decision 2's parse-failure arm, which is why the refusal is
/// [`DeriveError::Grammar`] and not a variant of its own: SA-2's sentence names that arm.
///
/// **Where this check does NOT live: the derivation layer.** `derive_with` does not apply it, and
/// that is a decision with a reason. `scene`, `cad` and `map` each enforce their own
/// `max_dsl_bytes` on their own entry and each pins the WORDS of that refusal in its corpus
/// golden — three different sentences, because each says what its own ceiling is for. A wall in
/// `derive_with` would run first and replace all three with one message, which would move three
/// goldens to say less. So the wall is the kind's, and the layer's job is to make sure the kind
/// has one: `derive::tests::every_transformer_refuses_an_answer_over_its_declared_dsl_ceiling`
/// and `palw-derive drill` feed every registered transformer an answer one byte over its declared
/// `max_dsl_bytes` and require a refusal that names the ceiling. A transformer with no wall fails
/// both, which is the fail-closed half of SA-2.
pub fn check_dsl_bytes(max_dsl_bytes: u64, bytes: &[u8]) -> Result<(), DeriveError> {
    if bytes.len() as u64 > max_dsl_bytes {
        return Err(DeriveError::Grammar(format!(
            "the answer is {} bytes, past the declared max_dsl_bytes of {max_dsl_bytes}; a bound exceeded is no object \
             (ADR-0078 SA-2)",
            bytes.len()
        )));
    }
    Ok(())
}

/// **The labels beside SA-2's numbers**, and the reason they are here and not in the manifest.
///
/// The three ceilings ARE manifest fields: each kind states them as constants of its own module,
/// spells them in its manifest literal, and enforces them itself before the work each one guards.
/// Two things SA-2 and SA-3 also need are not numbers a kind has had to choose yet — what
/// `max_steps` counts, and whether the transformer accepts uploaded inputs at all — and putting
/// them in the struct would make every kind spell `max_named_inputs: 0` to say what silence
/// already says. They live here, keyed by kind, and they reach `transformer_id` the way every
/// other constant of this crate does: through `source_tree_sha256`, which is a manifest field, so
/// changing one is a different transformer.
///
/// A kind that admits uploads adds one line here and the integrator has one place to look.
pub mod bounds {
    use super::NamedInputLimits;
    use kaspa_consensus_core::palw_derived_v1::kind;

    /// The unit `max_steps` counts, per kind. `canonical-dsl-byte` is the honest answer for a
    /// kind that has not named a unit of its own: the only work bound this layer has for it is
    /// its bytes.
    pub fn step_unit(kind_id: u16) -> &'static str {
        match kind_id {
            kind::CODE | kind::CONTRACT => "evm-gas",
            kind::IMAGE => "raster-pixel",
            kind::MUSIC => "midi-note",
            kind::SIMULATION => "simulation-step",
            // `scene` states its unit as a constant of its own module and its refusal message
            // reads from that constant; the row points at it rather than repeating the words,
            // because two spellings of one unit is how the manifest ends up describing a bound
            // the code no longer has.
            kind::SCENE => crate::kinds::scene::STEPS_UNIT,
            kind::CAD => "exact-predicate",
            kind::MAP => "cell-visit",
            _ => "canonical-dsl-byte",
        }
    }

    /// SA-3, per kind. Every shipped kind takes its whole input from the model's answer, so every
    /// row is zero and every upload is refused before it is read; a transformation kind
    /// (Decision 10) states its two numbers here.
    pub const fn named_input_limits(_kind_id: u16) -> NamedInputLimits {
        NamedInputLimits { max_inputs: 0, max_bytes: 0 }
    }
}

/// A transformer: a pure function from canonical DSL bytes to an artifact (Decision 3).
pub trait Transformer: Send + Sync {
    fn manifest(&self) -> TransformerManifest;
    /// Run on CANONICAL DSL bytes (the grammar's output). A transformer may assume the input is
    /// canonical and must refuse, not repair, anything else.
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError>;

    /// **SA-2's step bound, made enforceable BEFORE the run.** How much work the canonical DSL
    /// asks for, in the unit [`TransformerManifest::step_unit`] names, when the transformer can
    /// say so without doing the work. `derive_with` refuses `Some(w)` above the manifest's
    /// `max_steps` before calling [`Transformer::run`].
    ///
    /// The default is `None`, and it is an honest `None` rather than a `0`: this transformer
    /// does not expose a work count to the layer, and its `max_steps` documents the ceiling the
    /// kind enforces inside its own grammar. Implementing this is three lines in a kind module
    /// and it moves the refusal from "inside the run" to "before it", which is what SA-2 asks
    /// for on a DSL that can name work in a few bytes.
    fn declared_work(&self, _canonical_dsl: &[u8]) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod discipline {
    //! **ADR-0078 Decision 3 / X3, enforced rather than declared.**
    //!
    //! Decision 3 says a transformer "declares" integer or exact-rational arithmetic and no
    //! `f32`/`f64` on any path that reaches the output. A declaration nothing checks is the
    //! defect pattern this repository keeps re-recording, and `Discipline::Integer` in a manifest
    //! is exactly such a declaration: it is a string in a hash preimage, and nothing in the type
    //! system stops the transformer beside it from computing in floating point.
    //!
    //! What an in-tree test can honestly claim is a SOURCE-LEVEL check, and no more: no
    //! floating-point type name is spelled in the crate's non-test source. That is weaker than
    //! "no float instruction reaches the output" — a dependency could compute in float behind an
    //! integer-looking signature, and `as` casts through a generic could hide one — and it is
    //! strictly stronger than a declaration, because it is recomputed on every build over the
    //! files that are actually there. The two claims the check does NOT make are named here so
    //! that no reader mistakes it for a proof:
    //!
    //!   * it does not inspect the emitted instructions, and
    //!   * it does not follow calls out of this crate.
    //!
    //! X3's own instrument — the same DSL corpus, byte-identical on two architectures
    //! (`palw-derive drill --check`) — is the empirical half, and it is the half that would
    //! actually catch a float that got through, because a float on the output path is precisely
    //! a value two architectures may disagree on.
    //!
    //! Per-kind copies of this scan exist in `kinds/*.rs`. They are opt-in: a kind module that
    //! forgets to write one is unchecked, and a new kind is exactly the case where a discipline
    //! gate matters. This one enumerates the files on disk, so a kind cannot arrive without it.

    /// Everything under `src/`, in the same order the source-tree hash walks it.
    fn sources() -> Vec<String> {
        crate::source_tree::source_files(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
    }

    /// The part of a file that reaches the output, with the parts that cannot: everything before
    /// the trailing `#[cfg(test)]`, and on each line everything before a `//`.
    ///
    /// Both cuts are load-bearing and both were WRONG in the first draft of this scan, which is
    /// why they are spelled out. Test code may name a float — `fixed.rs` builds a binary32 bit
    /// pattern with integers and checks it against the hardware conversion, and that ORACLE is
    /// the point of the test. And a comment may name one: `fixed.rs`'s own first line quotes
    /// Decision 3's "no `f32`/`f64`", and a scan that reads its own subject's documentation as
    /// its subject fails on the file that proves it works. The line-comment cut is safe in this
    /// crate because no non-test source line here carries `//` inside a string literal (there is
    /// no URL and no block comment in `src/`); a scan that has to survive one would need a
    /// tokenizer, and the honest place to say so is here.
    pub(super) fn code_only(text: &str) -> String {
        // Comments go FIRST and the test cut second, not the other way round: `source_tree.rs`
        // explains in its header that "its `#[cfg(test)]` module is stripped in the build
        // script", and cutting on the marker before removing comments truncated that whole file
        // at its twelfth line — a scan that reads a sentence ABOUT a marker as the marker checks
        // nothing below it and reports green.
        let stripped: Vec<&str> = text
            .lines()
            .map(|l| match l.find("//") {
                Some(at) => &l[..at],
                None => l,
            })
            .collect();
        let body = stripped.join("\n");
        match body.find("#[cfg(test)]") {
            Some(at) => body[..at].to_string(),
            None => body,
        }
    }

    /// Split so this file does not trip its own scan.
    const FLOAT_TYPE_NAMES: [&str; 2] = [concat!("f", "32"), concat!("f", "64")];

    /// Whether `needle` appears in `line` as a TOKEN — not as a piece of a longer identifier.
    /// `fixed.rs` exports `f32_bits_exact`, whose whole purpose is to make the bit pattern of a
    /// binary32 with integer arithmetic; a substring scan reads that name as the very thing the
    /// function exists to avoid, and then the crate cannot both hold the function and pass its
    /// own discipline check.
    fn names_type(line: &str, needle: &str) -> bool {
        let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let bytes = line.as_bytes();
        let mut from = 0usize;
        while let Some(rel) = line[from..].find(needle) {
            let at = from + rel;
            let before_ok = at == 0 || !ident(bytes[at - 1] as char);
            let after = at + needle.len();
            let after_ok = after >= bytes.len() || !ident(bytes[after] as char);
            if before_ok && after_ok {
                return true;
            }
            from = at + 1;
        }
        false
    }

    #[test]
    fn no_floating_point_type_is_spelled_on_any_path_that_reaches_an_artifact() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut offenders = Vec::new();
        let mut scanned = 0usize;
        for rel in sources().into_iter().filter(|p| p.ends_with(".rs")) {
            let text = std::fs::read_to_string(root.join(&rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
            scanned += 1;
            for (n, line) in code_only(&text).lines().enumerate() {
                for needle in FLOAT_TYPE_NAMES {
                    if names_type(line, needle) {
                        offenders.push(format!("{rel}:{}: {}", n + 1, line.trim()));
                    }
                }
            }
        }
        let rust_files = sources().into_iter().filter(|p| p.ends_with(".rs")).count();
        assert_eq!(scanned, rust_files, "a Rust file the source-tree hash covers was not scanned");
        assert!(scanned >= 10, "the scan found almost no source: it is measuring itself, not the crate ({scanned} files)");
        assert!(
            offenders.is_empty(),
            "ADR-0078 Decision 3: a transformer declares integer or exact-rational arithmetic and no floating-point type on \
             any path that reaches the output. These lines spell one:\n{}",
            offenders.join("\n")
        );
    }

    /// The scan is fail-closed: it reads every file the source-tree hash covers, so a kind
    /// cannot arrive unscanned by forgetting to add a line anywhere.
    ///
    /// What this test does NOT do, because the first draft did and was wrong: infer a module
    /// from a transformer's name. `contract/evm/v1` lives in `kinds/code.rs` — one module holds
    /// the two transformers that share the `code/v1` grammar and the same machine — so
    /// "`<first path segment>.rs` exists" is false for a registered transformer, and the
    /// attribution buys nothing anyway: the scan is over files, not over transformers.
    #[test]
    fn the_scan_reads_every_file_the_source_tree_hash_covers() {
        let files = sources();
        let kinds: Vec<&String> = files.iter().filter(|p| p.starts_with("src/kinds/") && p.ends_with(".rs")).collect();
        assert!(kinds.iter().any(|p| *p == "src/kinds/mod.rs"));
        assert!(kinds.len() >= 8, "the kind table's seven rows plus mod.rs, found {kinds:?}");
        // Every registered transformer's module is one of the files above — stated as the true
        // form of the claim: some scanned file registers it, because `kinds::transformers()` is
        // built from those modules and from nothing else.
        assert!(!crate::registry::transformer_names().is_empty());
        assert!(files.iter().any(|p| p == "src/lib.rs"));
    }

    /// The scan would actually fail if a float appeared, and would not fail on the two shapes it
    /// must tolerate: proven on synthetic sources rather than by trusting that an assertion
    /// nobody has seen fire would fire.
    #[test]
    fn the_scan_catches_a_float_and_lets_a_name_a_comment_and_a_test_oracle_through() {
        let hit = |src: &str| code_only(src).lines().any(|l| FLOAT_TYPE_NAMES.iter().any(|n| names_type(l, n)));
        assert!(hit("pub fn area(w: i64) -> f64 { w as f64 }\n"), "a float on the output path must be caught");
        assert!(hit("let x: f32 = 1.0;\n"), "a binding must be caught");
        assert!(!hit("pub fn f32_bits_exact(v: i64) -> u32 { 0 }\n"), "an integer function whose NAME says binary32 is not a float");
        assert!(!hit("/// Decision 3: no `f32`/`f64` on any path.\npub fn f(x: i64) -> i64 { x }\n"), "a comment is not a path");
        assert!(!hit("pub fn area(w: i64) -> i64 { w }\n#[cfg(test)]\nmod tests { const O: f64 = 1.0; }\n"), "a test oracle may");
    }
}

#[cfg(test)]
mod host_posture {
    //! **ADR-0079 Decision 8 / S10, for this crate: a model's output is data on every path.**
    //!
    //! S10 says nothing in the tree executes, fetches, or shells out on the strength of model
    //! output. In this crate the model's output is the DSL, and the paths it reaches are the
    //! grammars and the transformers. The check is a source scan over every file the source-tree
    //! hash covers, for the spellings by which a Rust program starts a process, opens a socket or
    //! loads a library.
    //!
    //! Every such spelling in the crate is in ONE file, `kinds/code.rs`, and both of the doors it
    //! opens are ADR-0079 Decision 12's, not violations of S10:
    //!
    //!   * **the confined EVM runner** (`build_evm_v1_confined` → `palw-evm-runner`). ADR-0078
    //!     SA-1 requires model-written initcode to run in a separate process under Decision 5's
    //!     confinement, so the `code` and `contract` transformers DO start a program on the
    //!     strength of model output — which is precisely why the ADR gives it the narrowest cage
    //!     rather than forbidding it: an ephemeral tree, `env_clear`, a resident ceiling, a
    //!     derived deadline, and a child that holds no key, no claim and no answer. What S10
    //!     forbids is executing model output *in the process that holds them*, and the door that
    //!     does not do that is the one this crate keeps;
    //!   * **the pinned external-toolchain runner** (`run_external`), which ADR-0078 Decision 11
    //!     and ADR-0079 S11 govern. The binary it runs and every argument come from an
    //!     `ExternalToolchainManifest` an OPERATOR supplies, never from the DSL; it refuses a
    //!     binary whose SHA-256 is not the one the manifest pins, a host whose confinement backend
    //!     is `none`, and a host where a bond or wallet key is reachable. Nothing in the crate
    //!     calls it: the scan requires `run_external(` to appear in the non-test source exactly
    //!     once, at its own definition, so the day someone wires it to a `Transformer::run` this
    //!     test goes red before the drill does.
    //!
    //! `tests/derive_tree_guard.rs` holds the other half — that the in-process EVM entry point has
    //! exactly one caller and that it is the runner binary.
    //!
    //! `register()` returns no transformer for kind 19 (`agent`): ADR-0078 SA-6's planning mode
    //! produces a task graph as an ARTIFACT, and the assertion below is written so that
    //! registering an `agent` transformer that spawns anything fails here.

    fn sources() -> Vec<String> {
        crate::source_tree::source_files(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
    }

    fn code_only(text: &str) -> String {
        super::discipline::code_only(text)
    }

    /// The spellings by which a Rust program leaves its own address space. Assembled from pieces
    /// so this module's own text is not an offender.
    fn forbidden() -> Vec<&'static str> {
        vec![
            concat!("Command", "::new"),
            // ADR-0079 Decision 12 spawns through the host-security API rather than `Command::new`
            // — a scan that did not know that spelling would be blind to the doors this crate
            // actually opens.
            concat!("establish_", "confinement"),
            concat!(".command", "("),
            concat!("std::", "net::"),
            concat!("Tcp", "Stream"),
            concat!("Tcp", "Listener"),
            concat!("Udp", "Socket"),
            concat!("reqwest", "::"),
            concat!("ureq", "::"),
            concat!("libloading", "::"),
            concat!("dlopen", "("),
        ]
    }

    /// The one file allowed to name a spawn, and the two gate functions that must contain them.
    const RUNNER_FILE: &str = "src/kinds/code.rs";
    const RUNNER_FN: &str = "run_external";
    const GATES: &[&str] = &["pub fn build_evm_v1_confined", "pub fn run_external"];

    #[test]
    fn nothing_executes_fetches_or_shells_out_on_the_strength_of_model_output() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut offenders = Vec::new();
        let mut exempted = 0usize;
        for rel in sources() {
            let Ok(text) = std::fs::read_to_string(root.join(&rel)) else { continue };
            for (n, line) in code_only(&text).lines().enumerate() {
                for needle in forbidden() {
                    if line.contains(needle) {
                        if rel == RUNNER_FILE {
                            exempted += 1;
                        } else {
                            offenders.push(format!("{rel}:{}: {}", n + 1, line.trim()));
                        }
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "ADR-0079 Decision 8 / S10: nothing in the tree executes, fetches or shells out on the strength of model output. \
             These lines do:\n{}",
            offenders.join("\n")
        );
        assert!(
            exempted > 0,
            "the two gates' own spawns are the exemption, and they must still be in {RUNNER_FILE} — a crate that no \
             longer names a spawn anywhere has either lost the confined EVM runner or grown a third spelling this scan \
             cannot see"
        );
        // …and the exemption is only worth having while both gates are still gates.
        let gate_file = std::fs::read_to_string(root.join(RUNNER_FILE)).expect("the gate file is in the crate");
        for gate in GATES {
            assert!(gate_file.contains(gate), "ADR-0079 Decision 12: {RUNNER_FILE} must still hold `{gate}`");
        }
        assert!(
            gate_file.contains("reachable_signing_secrets"),
            "ADR-0079 Decision 12 / S11: the external toolchain's output is never executed on a host that holds a bond \
             or wallet key"
        );
        assert!(
            gate_file.contains("ConfinementBackend::None"),
            "ADR-0079 Decision 12 / S11: an external toolchain runs under a backend the host PROVED, or it does not run"
        );
    }

    #[test]
    fn the_one_spawn_in_the_crate_has_no_caller_and_no_transformer_reaches_it() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut mentions = Vec::new();
        for rel in sources() {
            let Ok(text) = std::fs::read_to_string(root.join(&rel)) else { continue };
            for (n, line) in code_only(&text).lines().enumerate() {
                if line.contains(&format!("{RUNNER_FN}(")) {
                    mentions.push(format!("{rel}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert_eq!(
            mentions.len(),
            1,
            "the pinned external toolchain runner must have exactly one mention in the non-test source — its own definition. \
             A second one is a caller, and a caller on a transformer's path is an S10 violation:\n{}",
            mentions.join("\n")
        );
        assert!(mentions[0].starts_with(&format!("{RUNNER_FILE}:")), "the runner moved: {}", mentions[0]);
        assert!(mentions[0].contains("pub fn"), "the single mention must be the definition, not a call: {}", mentions[0]);
    }

    /// SA-6: `agent` (kind 19) and Decision 10's planning mode produce artifacts only. This build
    /// registers no `agent` transformer; the assertion states the rule for the day one arrives,
    /// and the scan above already covers whatever module it would live in.
    #[test]
    fn a_task_graph_is_an_artifact_and_this_build_executes_none() {
        use kaspa_consensus_core::palw_derived_v1::kind;
        for (name, k, _) in crate::registry::transformer_names() {
            assert_ne!(
                k,
                kind::AGENT,
                "{name} registers kind 19: ADR-0078 SA-6 allows it to WRITE a task graph and nothing else — when this fires, \
                 check that its module names no spawn (the S10 scan above) before deleting this line"
            );
        }
    }
}
