//! The derivation itself (ADR-0078 §2): answer → canonical DSL → artifact → the object the chain
//! accepts, and the consumer's verification of the same chain of pure functions (Decision 5).

use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
use crate::registry::{grammar_by_id, transformer_by_id};
use crate::{Artifact, DeriveError, Grammar, Transformer};
use kaspa_consensus_core::palw_derived_v1::{PALW_DERIVED_V1_VERSION, PalwDerivedArtifactV1, derived_id_v1};
use kaspa_consensus_core::palw_v2::{output_commitment_v2, rendered_output_hash_v2};
use kaspa_hashes::Hash64;
use misaka_palw_base0::e2e_drill::PalwRcFamilyV1;

/// What binds a derivation to a claim — the chain-side facts the executor holds when the
/// inference finishes (ADR-0077 Decision 4's handoff).
#[derive(Clone, Debug)]
pub struct ClaimBinding {
    pub network_domain: Hash64,
    pub claim_id: Hash64,
    pub output_root: Hash64,
    pub executor_pubkey: Vec<u8>,
}

/// One derivation: the canonical DSL, the artifact, and the unsigned object that names both.
#[derive(Clone, Debug)]
pub struct Derivation {
    pub grammar_id: Hash64,
    pub transformer_id: Hash64,
    pub kind: u16,
    pub canonical_dsl: Vec<u8>,
    pub dsl_hash: Hash64,
    pub artifact: Artifact,
    pub artifact_hash: Hash64,
    pub object: PalwDerivedArtifactV1,
}

impl Derivation {
    pub fn derived_id(&self) -> Hash64 {
        derived_id_v1(&self.object)
    }
}

/// **SA-2, fail-closed: a transformer with a bound it did not fill cannot run.** Every ceiling is
/// a manifest field, so "declares none" is a zero — and a zero ceiling is refused by name rather
/// than read as "no limit", which is what a `0` in a bound field means to a reader who was not
/// looking for it.
///
/// The DSL ceiling itself is enforced by the KIND, on its own entry and before its own parser
/// (see [`crate::check_dsl_bytes`] for why the wall is there and not here); the two ceilings this
/// layer applies are the ones a kind can only bound by prediction — the work before the run, and
/// the artifact after it — and they are backstops behind each kind's own prediction, so they fire
/// only when a kind's accounting was wrong.
pub fn check_declared_bounds(manifest: &crate::TransformerManifest) -> Result<(), DeriveError> {
    for (field, value) in [
        ("max_dsl_bytes", manifest.max_dsl_bytes),
        ("max_artifact_bytes", manifest.max_artifact_bytes),
        ("max_steps", manifest.max_steps),
    ] {
        if value == 0 {
            return Err(DeriveError::Bound(format!(
                "transformer {} (kind {}) declares {field} 0; ADR-0078 SA-2 requires every transformer to declare \
                 max_dsl_bytes, max_artifact_bytes and max_steps before it may run",
                manifest.name, manifest.kind
            )));
        }
    }
    Ok(())
}

/// SA-2's step ceiling, BEFORE the run, for a transformer that can say what its input asks for
/// ([`Transformer::declared_work`]). A kind that predicts its own cost refuses earlier and in its
/// own words; this is the layer's copy of the same question, for one that does not.
fn check_work_bound(
    manifest: &crate::TransformerManifest,
    transformer: &dyn Transformer,
    canonical_dsl: &[u8],
) -> Result<(), DeriveError> {
    match transformer.declared_work(canonical_dsl) {
        Some(work) if work > manifest.max_steps => Err(DeriveError::Bound(format!(
            "the DSL asks for {work} {}, past the declared max_steps of {}; a bound exceeded is no object (ADR-0078 SA-2)",
            manifest.step_unit(),
            manifest.max_steps
        ))),
        _ => Ok(()),
    }
}

/// SA-2's artifact ceiling, on the bytes that came back and before any object names them. Every
/// shipped kind predicts its artifact's size from the DSL and refuses before building, so this
/// fires only when a kind's own prediction was wrong — which is exactly when a backstop is worth
/// having, and exactly why it is not the same check twice.
fn check_artifact_bound(manifest: &crate::TransformerManifest, artifact_bytes: usize) -> Result<(), DeriveError> {
    if artifact_bytes as u64 > manifest.max_artifact_bytes {
        return Err(DeriveError::Bound(format!(
            "{} built {artifact_bytes} bytes, past the declared max_artifact_bytes of {}; a bound exceeded is no object \
             (ADR-0078 SA-2), and this one was caught after the build because the transformer's own prediction let it through",
            manifest.name, manifest.max_artifact_bytes
        )));
    }
    Ok(())
}

/// Derive under an explicit grammar and transformer. `Err(Grammar)` is X4: the answer did not
/// parse, no object exists, and the claim is untouched. `Err(Bound)` is SA-2's arm of the same
/// sentence, and `Err(UnpublishedManifest)` is SA-5's.
pub fn derive_with(
    grammar: &dyn Grammar,
    transformer: &dyn Transformer,
    binding: &ClaimBinding,
    answer: &[u8],
) -> Result<Derivation, DeriveError> {
    let manifest = transformer.manifest();
    if manifest.grammar != grammar.name() {
        return Err(DeriveError::Transformer(format!(
            "transformer {} consumes grammar {}, not {}",
            manifest.name,
            manifest.grammar,
            grammar.name()
        )));
    }
    let transformer_id = transformer_id(&manifest);
    // SA-5, BEFORE anything runs: a derivation whose manifest is not published in this tree at
    // this id is one no consumer could ever verify (Decision 5 is exactly the promise that they
    // can), so it is refused rather than made. This is also the check that catches a transformer
    // handed to this function directly without ever being registered — `derive_named` cannot
    // reach one, but this entry point can, and an object naming an id nobody can resolve is the
    // unverifiable statement SA-5 says must not be storable.
    if !crate::registry::manifest_is_published(&transformer_id) {
        return Err(DeriveError::UnpublishedManifest(format!(
            "transformer {} names id {} and no manifest is published in this tree at it (ADR-0078 SA-5); \
             `palw-derive manifest --transformer <name>` prints the ones that are",
            manifest.name, transformer_id
        )));
    }
    check_declared_bounds(&manifest)?;
    let canonical_dsl = grammar.canonicalize(answer)?;
    let grammar_id = grammar_id_v1(grammar.name());
    let dsl_hash = dsl_hash_v1(&grammar_id, &canonical_dsl);
    check_work_bound(&manifest, transformer, &canonical_dsl)?;
    let artifact = transformer.run(&canonical_dsl)?;
    if artifact.bytes.is_empty() {
        return Err(DeriveError::Transformer("the transformer produced no bytes".into()));
    }
    check_artifact_bound(&manifest, artifact.bytes.len())?;
    let artifact_hash = artifact_hash_v1(&artifact.bytes);
    let object = PalwDerivedArtifactV1 {
        version: PALW_DERIVED_V1_VERSION,
        network_domain: binding.network_domain,
        claim_id: binding.claim_id,
        output_root: binding.output_root,
        grammar_id,
        transformer_id,
        kind: manifest.kind,
        dsl_hash,
        artifact_hash,
        artifact_bytes: artifact.bytes.len() as u64,
        executor_pubkey: binding.executor_pubkey.clone(),
    };
    Ok(Derivation { grammar_id, transformer_id, kind: manifest.kind, canonical_dsl, dsl_hash, artifact, artifact_hash, object })
}

/// Derive under the registry's transformer of the given name.
pub fn derive_named(transformer_name: &str, binding: &ClaimBinding, answer: &[u8]) -> Result<Derivation, DeriveError> {
    let transformer = crate::registry::transformer_by_name(transformer_name)
        .ok_or_else(|| DeriveError::UnknownTransformer(transformer_name.to_string()))?;
    let grammar_name = transformer.manifest().grammar;
    let grammar =
        crate::registry::grammar_by_name(grammar_name).ok_or_else(|| DeriveError::UnknownGrammar(grammar_name.to_string()))?;
    derive_with(grammar, transformer, binding, answer)
}

/// What a consumer checks over the bytes they hold (Decision 5, X6): from the answer bytes and
/// the object alone, recompute `dsl_hash` and `artifact_hash` and `artifact_bytes`, and demand
/// equality. `output_root` is the third of X6's recomputations and is [`verify_output_root`],
/// because it takes inputs the answer bytes do not carry — the job's context hash and the
/// family whose rendered-hash rule applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verification {
    pub dsl_hash_matches: bool,
    pub artifact_hash_matches: bool,
    pub artifact_bytes_matches: bool,
    /// **X8**: the object's `kind` against the manifest behind its `transformer_id`. The chain
    /// checks `kind != 0` and interprets nothing else, so this disagreement is not something a
    /// node can catch — "an object whose kind disagrees with its transformer's manifest is a
    /// false object under Decision 5 — demonstrable by anyone holding the manifest", and this
    /// field is that demonstration.
    pub kind_matches: bool,
    pub recomputed_dsl_hash: Hash64,
    pub recomputed_artifact_hash: Hash64,
    pub recomputed_artifact_bytes: u64,
    pub manifest_kind: u16,
}

impl Verification {
    pub fn all_match(&self) -> bool {
        self.dsl_hash_matches && self.artifact_hash_matches && self.artifact_bytes_matches && self.kind_matches
    }

    /// Which fields disagreed, BY NAME. A verdict of "MISMATCH" tells a reader that the object
    /// is false; it does not tell them where to look, and "the derivation is wrong somewhere" is
    /// the shape of report this repository keeps having to re-investigate. Empty when everything
    /// matched.
    pub fn mismatches(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.dsl_hash_matches {
            out.push("dsl_hash");
        }
        if !self.artifact_hash_matches {
            out.push("artifact_hash");
        }
        if !self.artifact_bytes_matches {
            out.push("artifact_bytes");
        }
        if !self.kind_matches {
            out.push("kind");
        }
        out
    }
}

/// **X6's third recomputation: the family's rendered-output hash.** Each shipped family's rule is
/// a keyed hash of the ids (BASE-0 has no tokenizer, so its rendering is empty and says so), and
/// the family is named with [`PalwRcFamilyV1`] rather than a string of this crate's own so that
/// there is exactly one spelling of a family name in the tree.
pub fn rendered_output_hash_for_family(family: PalwRcFamilyV1, output_token_ids: &[u32]) -> Hash64 {
    match family {
        // ADR-0078 X6 over the floor class: BASE-0 renders nothing, and the empty rendering is
        // the honest statement of that (the same call `misaka_palw_base0::produce` makes).
        PalwRcFamilyV1::Base0 => rendered_output_hash_v2(&[]),
        PalwRcFamilyV1::Qwen36 => misaka_palw_base0::qwen36_backend::rendered_output_hash_v1(output_token_ids),
        // **The fused graph renders exactly as the unfused one does**, and that is a fact about
        // what a family is rather than a convenience. `PalwRcFamilyV1` distinguishes GRAPHS,
        // because a class is its graph and the court must know which one it is trying. Rendering
        // is a property of the TOKENIZER and the model's vocabulary, which the fusion does not
        // touch — it replaces attention's scores/softmax/values nodes with one fused node and
        // changes no output ids. So the two A16 rows share this call, and a reader who expects the
        // arms to be one-per-graph should know the split is deliberate and the sharing is checked
        // by `the_fused_family_renders_as_the_unfused_one` below.
        PalwRcFamilyV1::Qwen25A16 | PalwRcFamilyV1::Qwen25A16V5 => {
            misaka_palw_base0::qwen25_a16_backend::rendered_output_hash_v1(output_token_ids)
        }
    }
}

/// **X6: `output_root` from the answer's ids.** ADR-0078 Decision 2 records the correction the
/// ADR's first draft got wrong — `output_root` is NOT a hash over the ids alone but
/// `output_commitment_v2(job_context_hash, ids, family_rendered_hash)`, three inputs — and this
/// is the one function that spells it for a consumer. All three are values the person holding the
/// answer has: the ids are the answer, the job's context hash is the public value the gateway
/// returns beside it, and the family is the class they asked.
pub fn recompute_output_root(family: PalwRcFamilyV1, job_context_hash: &Hash64, output_token_ids: &[u32]) -> Hash64 {
    output_commitment_v2(job_context_hash, output_token_ids, &rendered_output_hash_for_family(family, output_token_ids))
}

/// Whether the object's `output_root` is the one those ids imply — the cross-check that ties the
/// derivation to the claim (Decision 4: "a cross-check, not a second source"). `false` is a
/// demonstrable false object under Decision 5.
pub fn verify_output_root(
    object: &PalwDerivedArtifactV1,
    family: PalwRcFamilyV1,
    job_context_hash: &Hash64,
    output_token_ids: &[u32],
) -> bool {
    recompute_output_root(family, job_context_hash, output_token_ids) == object.output_root
}

/// Re-run the derivation named by the object over `answer` and compare. `Err` means the
/// derivation could not even be re-run (unknown ids, a grammar refusal) — which is itself a
/// demonstrable mismatch: the object names a computation the answer does not admit.
pub fn verify(object: &PalwDerivedArtifactV1, answer: &[u8]) -> Result<Verification, DeriveError> {
    let grammar = grammar_by_id(&object.grammar_id).ok_or_else(|| DeriveError::UnknownGrammar(object.grammar_id.to_string()))?;
    let transformer =
        transformer_by_id(&object.transformer_id).ok_or_else(|| DeriveError::UnknownTransformer(object.transformer_id.to_string()))?;
    let manifest = transformer.manifest();
    // SA-2 protects the CONSUMER too, and by the same reasoning: they are about to parse and run
    // over bytes a stranger handed them because an object on the chain pointed at those bytes.
    // The kind's own DSL ceiling refuses inside `canonicalize`, on the same entry the executor
    // took; these two are the layer's backstops.
    check_declared_bounds(&manifest)?;
    let canonical_dsl = grammar.canonicalize(answer)?;
    let recomputed_dsl_hash = dsl_hash_v1(&object.grammar_id, &canonical_dsl);
    check_work_bound(&manifest, transformer, &canonical_dsl)?;
    let artifact = transformer.run(&canonical_dsl)?;
    check_artifact_bound(&manifest, artifact.bytes.len())?;
    let recomputed_artifact_hash = artifact_hash_v1(&artifact.bytes);
    Ok(Verification {
        dsl_hash_matches: recomputed_dsl_hash == object.dsl_hash,
        artifact_hash_matches: recomputed_artifact_hash == object.artifact_hash,
        artifact_bytes_matches: artifact.bytes.len() as u64 == object.artifact_bytes,
        kind_matches: object.kind == manifest.kind,
        recomputed_dsl_hash,
        recomputed_artifact_hash,
        recomputed_artifact_bytes: artifact.bytes.len() as u64,
        manifest_kind: manifest.kind,
    })
}

/// Verify a derivation against artifact BYTES the consumer holds (they were handed the GLB, not
/// only the JSON): the artifact's hash and size against the object.
pub fn verify_artifact_bytes(object: &PalwDerivedArtifactV1, artifact: &[u8]) -> bool {
    artifact_hash_v1(artifact) == object.artifact_hash && artifact.len() as u64 == object.artifact_bytes
}

// -------------------------------------------------------------------------------------------
// SA-3 — the inputs a transformation names by hash, which are bytes a stranger uploads
// -------------------------------------------------------------------------------------------
//
// ADR-0078 Decision 10's transformation mode (image → 3D, audio → MIDI, CSV → database) takes a
// second input the model did not read as tokens: "the DSL names them by hash and the transformer
// takes them as a second input", and X9 keeps `dsl_hash` covering the naming. SA-3 states what
// that means for a host: those bytes arrive from whoever asked for the job, so their size is
// bounded, they are held only for the job's life, and a hash the consumer cannot resolve is a
// demonstrable gap rather than an error the chain sees.
//
// This is the LIBRARY half — the bound and the check, so that both the gateway (which accepts the
// upload) and a consumer (who re-runs the derivation with the same bytes) apply one rule. The
// gateway's own wiring — where the bytes are read, how long the job lives — is not this crate's.

/// The key under which an uploaded input is content-named. Its own domain: an input is not an
/// artifact and not a DSL, and three things sharing one domain is how a preimage from one lane
/// gets replayed into another.
pub const NAMED_INPUT_DOMAIN: &[u8] = b"misaka-palw/derive/named-input/v1";

/// `H_key(len ‖ bytes)` — the name a DSL uses for an input it did not carry.
pub fn named_input_hash_v1(bytes: &[u8]) -> Hash64 {
    let mut st = blake2b_simd::Params::new().hash_length(64).key(NAMED_INPUT_DOMAIN).to_state();
    st.update(&(bytes.len() as u64).to_le_bytes());
    st.update(bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(st.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// One uploaded input, held for the job's life and no longer. Dropping it wipes the buffer: the
/// bytes belong to whoever uploaded them, and a host that keeps a stranger's file in freed memory
/// has kept it. (This is hygiene, not a guarantee — a reallocation during `Vec` growth can leave a
/// copy behind, and only a buffer sized once at read time avoids that. `NamedInput::new` sizes it
/// once for exactly that reason.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedInput {
    /// The name the DSL uses for this input.
    pub name: String,
    bytes: Vec<u8>,
}

impl NamedInput {
    /// Take ownership of the bytes of one upload, in a buffer sized once.
    pub fn new(name: impl Into<String>, bytes: &[u8]) -> Self {
        let mut buf = Vec::with_capacity(bytes.len());
        buf.extend_from_slice(bytes);
        NamedInput { name: name.into(), bytes: buf }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The content name these bytes have.
    pub fn hash(&self) -> Hash64 {
        named_input_hash_v1(&self.bytes)
    }
}

impl Drop for NamedInput {
    fn drop(&mut self) {
        self.bytes.iter_mut().for_each(|b| *b = 0);
    }
}

/// **The bound, before the bytes are read.** A host deciding whether to accept an upload of
/// `offered_bytes` asks here, and gets a refusal it can return without having buffered anything.
/// `Ok(())` does not promise the whole set will fit — that is [`check_named_inputs`] — only that
/// this one may be read.
///
/// The bounds are the argument rather than the manifest so that the number a host enforces is the
/// number it was handed: a check that looked its own limit up would be a second reader of the
/// table, and `who` (the transformer's name) is only for the message.
pub fn check_offered_named_input(limits: crate::NamedInputLimits, who: &str, offered_bytes: u64) -> Result<(), DeriveError> {
    if limits.max_inputs == 0 {
        return Err(DeriveError::Bound(format!("transformer {who} takes no hash-named inputs; it declares max_inputs 0")));
    }
    if offered_bytes > limits.max_bytes {
        return Err(DeriveError::Bound(format!(
            "an input of {offered_bytes} bytes was offered to {who}, which declares max_bytes {}",
            limits.max_bytes
        )));
    }
    Ok(())
}

/// **The check, over the set the job actually holds.** `declared` is what the DSL named, in the
/// DSL's own order; `held` is what the host has. Every declared name must be present exactly
/// once, with bytes whose hash is the declared one; nothing may be held that the DSL did not
/// name; and the count and the total size must be within the manifest's bounds.
///
/// Refusing an input the DSL did not name is not pedantry: a transformer is a pure function of
/// the canonical DSL and the bytes it names (X9), so an extra input is either dead weight the
/// host was made to carry or a value some future code path could read — and a derivation that
/// depended on it would not be reproducible from `dsl_hash`.
pub fn check_named_inputs(
    limits: crate::NamedInputLimits,
    who: &str,
    declared: &[(String, Hash64)],
    held: &[NamedInput],
) -> Result<(), DeriveError> {
    if held.len() as u64 > u64::from(limits.max_inputs) {
        return Err(DeriveError::Bound(format!(
            "{} inputs were held for {who}, which declares max_inputs {}",
            held.len(),
            limits.max_inputs
        )));
    }
    let total: u64 = held.iter().map(|i| i.len() as u64).sum();
    if total > limits.max_bytes {
        return Err(DeriveError::Bound(format!(
            "the held inputs are {total} bytes in total; {who} declares max_bytes {}",
            limits.max_bytes
        )));
    }
    let mut names = std::collections::BTreeSet::new();
    for (name, _) in declared {
        if !names.insert(name.as_str()) {
            return Err(DeriveError::Mismatch(format!("the DSL names the input {name:?} twice")));
        }
    }
    for input in held {
        if !names.contains(input.name.as_str()) {
            return Err(DeriveError::Mismatch(format!(
                "an input named {:?} is held and the DSL does not name it; a transformer reads only what dsl_hash covers (X9)",
                input.name
            )));
        }
    }
    for (name, want) in declared {
        let mut matching = held.iter().filter(|i| &i.name == name);
        let Some(input) = matching.next() else {
            // SA-3: "a hash the consumer cannot resolve is a demonstrable gap, not an error the
            // chain sees" — so it is a refusal here, named, and nothing is recorded anywhere.
            return Err(DeriveError::Mismatch(format!("the DSL names the input {name:?} ({want}) and nobody can resolve it")));
        };
        if matching.next().is_some() {
            return Err(DeriveError::Mismatch(format!("two different byte strings are held for the input {name:?}")));
        }
        let got = input.hash();
        if got != *want {
            return Err(DeriveError::Mismatch(format!("the input {name:?} hashes to {got}, and the DSL names {want}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_derived_v1::{PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN, check_derived_shape_v1, kind};

    /// The corpus's smallest answer, and the same bytes with whitespace and key order moved —
    /// a grammar canonicalizes both to one string, so both derive to one object.
    const ANSWER: &[u8] = br#"{"v":1,"ppq":480,"tempo_us_per_quarter":500000,"time_signature":[4,4],
        "tracks":[{"name":"lead","channel":0,"program":0,
                   "notes":[{"pitch":60,"velocity":100,"onset":0,"duration":480}]}]}"#;
    const ANSWER_REORDERED: &[u8] = br#"{"tracks":[{"program":0,"notes":[{"duration":480,"onset":0,"velocity":100,"pitch":60}],
        "channel":0,"name":"lead"}],"time_signature":[4,4],"tempo_us_per_quarter":500000,"ppq":480,"v":1}"#;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn binding() -> ClaimBinding {
        ClaimBinding {
            network_domain: h(0x01),
            claim_id: h(0x02),
            output_root: h(0x03),
            executor_pubkey: vec![0x11; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
        }
    }

    /// **§2's chain, end to end**: answer → canonical DSL → artifact → the object naming both,
    /// and the object is one the chain's stateless shape check accepts.
    #[test]
    fn a_derivation_names_what_it_made_and_the_object_passes_the_shape_check() {
        let d = derive_named("music/smf/v1", &binding(), ANSWER).expect("the corpus answer derives");
        assert_eq!(d.kind, kind::MUSIC);
        assert_eq!(d.grammar_id, grammar_id_v1("music/v1"));
        assert_eq!(d.dsl_hash, dsl_hash_v1(&d.grammar_id, &d.canonical_dsl));
        assert_eq!(d.artifact_hash, artifact_hash_v1(&d.artifact.bytes));
        assert_eq!(d.object.artifact_bytes, d.artifact.bytes.len() as u64);
        assert_eq!(d.object.claim_id, h(0x02));
        assert_eq!(d.object.output_root, h(0x03), "the object carries the claim's output_root, it does not invent one");
        assert_eq!(check_derived_shape_v1(&d.object), Ok(()));
        assert_eq!(d.derived_id(), derived_id_v1(&d.object));
    }

    /// Decision 2: the canonicalizer is whitespace, key order and number form — nothing
    /// semantic. Two spellings of one answer are one derivation, down to the derived id.
    #[test]
    fn key_order_and_whitespace_do_not_change_the_derivation() {
        let a = derive_named("music/smf/v1", &binding(), ANSWER).unwrap();
        let b = derive_named("music/smf/v1", &binding(), ANSWER_REORDERED).unwrap();
        assert_eq!(a.canonical_dsl, b.canonical_dsl);
        assert_eq!(a.dsl_hash, b.dsl_hash);
        assert_eq!(a.artifact.bytes, b.artifact.bytes);
        assert_eq!(a.derived_id(), b.derived_id());
    }

    /// **X4: a parse failure yields no object and nothing else.** The error is the whole result;
    /// there is no partial derivation to observe, and no `Derivation` value exists to carry one.
    #[test]
    fn a_grammar_refusal_yields_no_object_and_names_the_grammar() {
        for (why, answer) in [
            ("not JSON", &b"not json at all"[..]),
            ("a float", br#"{"v":1,"ppq":480.5}"#),
            ("a duplicate key", br#"{"v":1,"v":2}"#),
            ("the wrong schema", br#"{"v":1,"tracks":"lead"}"#),
        ] {
            let err = derive_named("music/smf/v1", &binding(), answer).expect_err(why);
            assert!(matches!(err, DeriveError::Grammar(_)), "{why} must refuse as a grammar failure, got {err:?}");
        }
    }

    /// A transformer and a grammar that do not belong together is refused before anything runs —
    /// the manifest names the grammar it consumes, and that name is checked, not assumed.
    #[test]
    fn a_transformer_refuses_a_grammar_its_manifest_does_not_name() {
        let music = crate::registry::transformer_by_name("music/smf/v1").unwrap();
        let Some(other) = crate::registry::grammar_names().into_iter().find(|g| *g != music.manifest().grammar) else {
            return; // a build with one grammar has nothing to cross
        };
        let grammar = crate::registry::grammar_by_name(other).unwrap();
        let err = derive_with(grammar, music, &binding(), ANSWER).expect_err("a crossed pair must refuse");
        assert!(matches!(err, DeriveError::Transformer(_)), "got {err:?}");
    }

    /// **X6, the two recomputations the answer alone supports**: a consumer holding the answer
    /// bytes and the object re-runs the derivation and reaches the object's values.
    #[test]
    fn a_consumer_holding_the_answer_reaches_the_objects_values() {
        let d = derive_named("music/smf/v1", &binding(), ANSWER).unwrap();
        let v = verify(&d.object, ANSWER).expect("re-runs");
        assert!(v.all_match(), "{v:?}");
        assert!(v.mismatches().is_empty());
        assert_eq!(v.recomputed_dsl_hash, d.dsl_hash);
        assert_eq!(v.recomputed_artifact_hash, d.artifact_hash);
        assert!(verify_artifact_bytes(&d.object, &d.artifact.bytes));
        // and the same check over the reordered spelling of the same answer
        assert!(verify(&d.object, ANSWER_REORDERED).unwrap().all_match());
    }

    /// **Decision 5: a false object is publicly demonstrable, and the demonstration says WHICH
    /// field is false.** Each mutation below is a lie a executor could tell; each is caught, and
    /// `mismatches()` names exactly the field that moved.
    #[test]
    fn every_lie_in_the_object_is_caught_and_named() {
        let d = derive_named("music/smf/v1", &binding(), ANSWER).unwrap();
        let mut lie = d.object.clone();
        lie.dsl_hash = h(0xEE);
        assert_eq!(verify(&lie, ANSWER).unwrap().mismatches(), ["dsl_hash"]);

        let mut lie = d.object.clone();
        lie.artifact_hash = h(0xEE);
        assert_eq!(verify(&lie, ANSWER).unwrap().mismatches(), ["artifact_hash"]);

        let mut lie = d.object.clone();
        lie.artifact_bytes += 1;
        assert_eq!(verify(&lie, ANSWER).unwrap().mismatches(), ["artifact_bytes"]);

        // A different answer is every hash at once.
        let other = br#"{"v":1,"ppq":480,"tempo_us_per_quarter":500000,"time_signature":[4,4],
            "tracks":[{"name":"lead","channel":0,"program":0,
                       "notes":[{"pitch":61,"velocity":100,"onset":0,"duration":480}]}]}"#;
        assert_eq!(verify(&d.object, other).unwrap().mismatches(), ["dsl_hash", "artifact_hash"]);

        // An object naming a transformer this build does not have cannot be re-run at all —
        // which is itself the demonstration, not a pass.
        let mut foreign = d.object.clone();
        foreign.transformer_id = h(0xAB);
        assert!(matches!(verify(&foreign, ANSWER), Err(DeriveError::UnknownTransformer(_))));
        let mut foreign = d.object.clone();
        foreign.grammar_id = h(0xAB);
        assert!(matches!(verify(&foreign, ANSWER), Err(DeriveError::UnknownGrammar(_))));

        // Artifact BYTES that are not the ones the object names.
        assert!(!verify_artifact_bytes(&d.object, b"a different file"));
    }

    /// **X6's third recomputation**: `output_root` is the THREE-input commitment, not a hash over
    /// the ids. The negative half is the one that matters — a two-input spelling would agree with
    /// the code on a fixed context and diverge everywhere else, so the test moves each input in
    /// turn and demands the root move with it.
    #[test]
    fn output_root_is_the_three_input_commitment_and_every_input_moves_it() {
        let ctx = h(0x42);
        let ids: Vec<u32> = vec![1, 2, 3, 5, 8];
        let mut seen = std::collections::BTreeSet::new();
        let mut seen_renderings = std::collections::BTreeSet::new();
        for family in PalwRcFamilyV1::ALL {
            let root = recompute_output_root(family, &ctx, &ids);
            assert_eq!(root, output_commitment_v2(&ctx, &ids, &rendered_output_hash_for_family(family, &ids)));
            // **Distinctness across families is NOT the commitment's contract, and asserting it
            // here was incidental.** `output_commitment_v2`'s three inputs are the job context
            // hash, the generated ids and the rendered-output hash — the family is deliberately
            // not one of them. So two families sharing a root means the context, the tokens AND
            // the rendering all matched: the same output, produced twice. That is a fact about
            // the answer, not a collision.
            //
            // It held only while every family happened to render differently, and the fused and
            // unfused A16 rows do not — rendering is the tokenizer's, and the fusion changes no
            // output id. Making the root depend on the family instead would make a derivation
            // depend on WHO CERTIFIED IT, which breaks the property this release is about: the
            // model's answer IS the artifact's source, and a stranger recomputes from the answer
            // alone. So the check is narrowed to families that render differently, and what it no
            // longer covers is said here rather than left to be rediscovered.
            let renders = rendered_output_hash_for_family(family, &ids);
            if seen_renderings.insert(renders) {
                assert!(seen.insert(root), "{} shares a root with a family that renders DIFFERENTLY", family.name());
            }
            assert_ne!(root, recompute_output_root(family, &h(0x43), &ids), "the job's context hash is an input");
            assert_ne!(root, recompute_output_root(family, &ctx, &[1, 2, 3, 5, 9]), "the ids are an input");
        }
        // BASE-0 renders nothing; the two tokenizer families render the ids, and differently.
        assert_eq!(rendered_output_hash_for_family(PalwRcFamilyV1::Base0, &ids), rendered_output_hash_v2(&[]));
        assert_eq!(rendered_output_hash_for_family(PalwRcFamilyV1::Base0, &[]), rendered_output_hash_v2(&[]));
        assert_ne!(
            rendered_output_hash_for_family(PalwRcFamilyV1::Qwen36, &ids),
            rendered_output_hash_for_family(PalwRcFamilyV1::Qwen25A16, &ids)
        );
    }

    /// The cross-check Decision 4 makes at acceptance, from the consumer's side: an object whose
    /// `output_root` is not the one the answer's ids imply is false, and one whose is, is not.
    #[test]
    fn verify_output_root_accepts_the_claims_root_and_refuses_another() {
        let ctx = h(0x42);
        let ids: Vec<u32> = vec![7, 7, 7];
        let family = PalwRcFamilyV1::Qwen36;
        let mut b = binding();
        b.output_root = recompute_output_root(family, &ctx, &ids);
        let d = derive_named("music/smf/v1", &b, ANSWER).unwrap();
        assert!(verify_output_root(&d.object, family, &ctx, &ids));
        assert!(!verify_output_root(&d.object, family, &ctx, &[7, 7, 8]), "other ids are another claim");
        assert!(!verify_output_root(&d.object, family, &h(0x99), &ids), "another job is another claim");
        assert!(!verify_output_root(&d.object, PalwRcFamilyV1::Qwen25A16, &ctx, &ids), "another family is another claim");
    }

    // ---------------------------------------------------------------------------------------
    // SA-2, SA-3, SA-5 and X8 — the security amendment's arms, each with the wiring proved
    // ---------------------------------------------------------------------------------------

    /// A transformer that carries a REGISTERED transformer's manifest (so SA-5 is satisfied and
    /// the test is about SA-2 and nothing else), reports whatever work it is told to, and records
    /// whether it was run.
    struct WorkSpy {
        inner: &'static dyn Transformer,
        work: Option<u64>,
        oversized_artifact: bool,
        ran: std::sync::atomic::AtomicBool,
    }

    impl WorkSpy {
        fn of(name: &str) -> Self {
            WorkSpy {
                inner: crate::registry::transformer_by_name(name).expect("registered"),
                work: None,
                oversized_artifact: false,
                ran: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    impl Transformer for WorkSpy {
        fn manifest(&self) -> crate::TransformerManifest {
            self.inner.manifest()
        }
        fn declared_work(&self, _canonical_dsl: &[u8]) -> Option<u64> {
            self.work
        }
        fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
            self.ran.store(true, std::sync::atomic::Ordering::SeqCst);
            let mut artifact = self.inner.run(dsl)?;
            if self.oversized_artifact {
                let ceiling = self.manifest().max_artifact_bytes as usize;
                artifact.bytes = vec![0x4d; ceiling + 1];
            }
            Ok(artifact)
        }
    }

    fn music() -> &'static dyn Transformer {
        crate::registry::transformer_by_name("music/smf/v1").expect("registered")
    }

    /// **SA-2's walls, and which one runs where.** The DSL ceiling is the KIND's, on its own
    /// entry and before its own parser — every registered transformer is fed an answer one byte
    /// over its declared `max_dsl_bytes` here and must refuse it naming the ceiling, which is the
    /// fail-closed half: a transformer that ships without that wall fails this test. The step and
    /// artifact ceilings are the layer's backstops, proved below on a transformer that carries a
    /// published manifest and lies about its work.
    #[test]
    fn every_transformer_refuses_an_answer_over_its_declared_dsl_ceiling() {
        for (name, _, _) in crate::registry::transformer_names() {
            let m = crate::registry::transformer_by_name(name).unwrap().manifest();
            assert!(check_declared_bounds(&m).is_ok(), "{name} ships a zero bound");
            let mut over = vec![b' '; m.max_dsl_bytes as usize + 1];
            over[0] = b'{';
            *over.last_mut().unwrap() = b'}';
            let err = derive_named(name, &binding(), &over).unwrap_err();
            assert!(err.is_refusal(), "{name}: {err:?}");
            assert!(
                err.to_string().contains(&m.max_dsl_bytes.to_string()) && err.to_string().contains(&over.len().to_string()),
                "{name}: SA-2's refusal must name the ceiling and the size it was handed: {err}"
            );
        }
    }

    #[test]
    fn the_step_and_artifact_ceilings_are_checked_before_and_after_the_run() {
        let m = music().manifest();
        let grammar = crate::registry::grammar_by_name("music/v1").unwrap();

        // the step ceiling, BEFORE the run
        let over = WorkSpy { work: Some(m.max_steps + 1), ..WorkSpy::of("music/smf/v1") };
        let err = derive_with(grammar, &over, &binding(), ANSWER).expect_err("more work than the manifest allows is no object");
        assert!(matches!(err, DeriveError::Bound(_)), "{err:?}");
        assert!(err.to_string().contains(m.step_unit()) && err.to_string().contains("max_steps"), "{err}");
        assert!(err.is_refusal(), "SA-2 says exceeding a bound is the parse-failure arm");
        assert!(!over.ran.load(std::sync::atomic::Ordering::SeqCst), "the transformer ran work the bound had already refused");

        // exactly at the ceiling is fine: a bound refuses what is over it and nothing else
        let at = WorkSpy { work: Some(m.max_steps), ..WorkSpy::of("music/smf/v1") };
        assert!(derive_with(grammar, &at, &binding(), ANSWER).is_ok());

        // the artifact ceiling, on the bytes that came back and before an object names them
        let big = WorkSpy { oversized_artifact: true, ..WorkSpy::of("music/smf/v1") };
        let err = derive_with(grammar, &big, &binding(), ANSWER).expect_err("an artifact over the ceiling is no object");
        assert!(matches!(err, DeriveError::Bound(_)), "{err:?}");
        assert!(err.to_string().contains("max_artifact_bytes"), "{err}");

        // and SA-2's fail-closed arm: a manifest with a bound nobody filled cannot run at all
        let zeroed = crate::TransformerManifest { max_steps: 0, ..m };
        let err = check_declared_bounds(&zeroed).expect_err("a zero bound is not 'no limit'");
        assert!(err.to_string().contains("max_steps 0"), "{err}");
    }

    /// SA-2 protects the consumer as well as the executor: `verify` re-enters through the same
    /// grammar, so the kind's own ceiling refuses the same bytes on the consumer's side — an
    /// object cannot be used to make a verifier parse something the executor would have refused.
    #[test]
    fn verification_refuses_the_same_oversized_bytes_the_derivation_would_have() {
        let d = derive_named("music/smf/v1", &binding(), ANSWER).unwrap();
        let m = music().manifest();
        let oversized = vec![b' '; m.max_dsl_bytes as usize + 1];
        let err = verify(&d.object, &oversized).expect_err("a verifier refuses the same bytes");
        assert!(err.is_refusal(), "{err:?}");
        assert!(err.to_string().contains(&m.max_dsl_bytes.to_string()), "{err}");
    }

    /// **SA-5: an unpublished manifest is refused BY NAME, before anything runs.** The transformer
    /// below is real, pure and correct; the only thing wrong with it is that no manifest is
    /// published in this tree at the id it would name, so nobody could ever check a derivation of
    /// it — and Decision 5's promise is exactly that they could.
    #[test]
    fn a_transformer_whose_manifest_this_tree_does_not_publish_cannot_derive() {
        struct Unpublished(&'static str);
        impl Transformer for Unpublished {
            fn manifest(&self) -> crate::TransformerManifest {
                crate::TransformerManifest { name: self.0, ..music().manifest() }
            }
            fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
                music().run(dsl)
            }
        }
        let mine = Unpublished("music/smf/v9-not-shipped");
        let id = transformer_id(&mine.manifest());
        assert!(!crate::registry::manifest_is_published(&id));
        let err = derive_with(crate::registry::grammar_by_name("music/v1").unwrap(), &mine, &binding(), ANSWER)
            .expect_err("SA-5 refuses a derivation nobody could verify");
        match &err {
            DeriveError::UnpublishedManifest(why) => {
                assert!(why.contains("music/smf/v9-not-shipped"), "the refusal must name the transformer: {why}");
                assert!(why.contains(&id.to_string()), "the refusal must name the id nobody can resolve: {why}");
            }
            other => panic!("{other:?}"),
        }
        assert!(err.is_refusal());
        // The published one, with the identical code, derives.
        assert!(derive_with(crate::registry::grammar_by_name("music/v1").unwrap(), music(), &binding(), ANSWER).is_ok());
    }

    /// **X8: the chain interprets no kind, so the consumer must.** An object whose `kind`
    /// disagrees with the manifest behind its `transformer_id` passes `check_derived_shape_v1`
    /// (which only asks `kind != 0`) and is still a false object; `verify` names it.
    #[test]
    fn an_object_whose_kind_disagrees_with_its_manifest_is_named_false() {
        let d = derive_named("music/smf/v1", &binding(), ANSWER).unwrap();
        let mut lie = d.object.clone();
        lie.kind = kind::SCENE;
        assert_eq!(check_derived_shape_v1(&lie), Ok(()), "the chain accepts any non-zero kind, which is why this check exists");
        let v = verify(&lie, ANSWER).unwrap();
        assert_eq!(v.mismatches(), ["kind"]);
        assert_eq!(v.manifest_kind, kind::MUSIC);
        assert!(!v.all_match());
    }

    /// **SA-3: the bytes a stranger uploads are bounded, named by hash, and nothing else is
    /// held.** Every landed transformer declares `max_inputs = 0` — none of them takes a
    /// second input — so the fail-closed half is the one that runs today: an upload offered to
    /// any shipped transformer is refused before it is read.
    #[test]
    fn hash_named_inputs_are_refused_by_every_shipped_transformer_before_a_byte_is_read() {
        for (name, _, _) in crate::registry::transformer_names() {
            let m = crate::registry::transformer_by_name(name).unwrap().manifest();
            let limits = m.named_input_limits();
            let err = check_offered_named_input(limits, m.name, 1).expect_err("a shipped transformer takes no uploads");
            assert!(matches!(err, DeriveError::Bound(_)), "{name}: {err:?}");
            assert!(err.to_string().contains("max_inputs 0"), "{name}: {err}");
            assert!(check_named_inputs(limits, m.name, &[], &[]).is_ok(), "{name}: no inputs against no declaration is fine");
        }

        // The content name is a hash of the bytes and of nothing else, and a changed byte is a
        // changed name — which is the whole reason a DSL can name bytes it does not carry.
        let photo = NamedInput::new("photo", b"the bytes a stranger uploaded");
        assert_eq!(photo.hash(), named_input_hash_v1(b"the bytes a stranger uploaded"));
        assert_ne!(photo.hash(), named_input_hash_v1(b"the bytes a stranger uploaded."));
        // The length is in the preimage, so no concatenation of two inputs reads as another input.
        assert_ne!(named_input_hash_v1(b"ab"), named_input_hash_v1(b"a"));
        assert_ne!(named_input_hash_v1(b""), crate::ids::artifact_hash_v1(b""), "an input is not an artifact; the domains differ");
    }

    /// The set-level checks of SA-3, against bounds that DO admit inputs — the row a
    /// transformation kind will publish. An unresolvable hash, a substituted file, an extra file,
    /// a duplicate name, too many inputs and too many bytes are each refused, and each refusal
    /// says which.
    #[test]
    fn a_held_input_must_be_the_one_the_dsl_named_and_nothing_may_be_held_beside_it() {
        let admitting = crate::NamedInputLimits { max_inputs: 2, max_bytes: 16 };
        let who = "transform/spec/v1";
        let photo = NamedInput::new("photo", b"aaaa");
        let declared = vec![("photo".to_string(), photo.hash())];

        assert!(check_named_inputs(admitting, who, &declared, std::slice::from_ref(&photo)).is_ok(), "the named bytes are held");
        assert!(check_offered_named_input(admitting, who, 16).is_ok());
        assert!(check_offered_named_input(admitting, who, 17).is_err(), "an upload over the ceiling is refused unread");

        let refusal = |held: Vec<NamedInput>, declared: &[(String, Hash64)]| {
            check_named_inputs(admitting, who, declared, &held).expect_err("must be refused").to_string()
        };
        assert!(refusal(vec![NamedInput::new("photo", b"bbbb")], &declared).contains("hashes to"), "a substituted file");
        assert!(
            refusal(vec![NamedInput::new("photo", b"aaaa"), NamedInput::new("extra", b"cccc")], &declared)
                .contains("does not name it"),
            "an extra file"
        );
        assert!(refusal(vec![], &declared).contains("nobody can resolve"), "SA-3's unresolvable hash is a named gap");
        let twice = vec![("photo".to_string(), photo.hash()), ("photo".to_string(), photo.hash())];
        assert!(refusal(vec![], &twice).contains("twice"), "a DSL cannot name one input twice");

        let three: Vec<(String, Hash64)> = ["a", "b", "c"].iter().map(|n| (n.to_string(), NamedInput::new(*n, b"x").hash())).collect();
        let held: Vec<NamedInput> = ["a", "b", "c"].iter().map(|n| NamedInput::new(*n, b"x")).collect();
        assert!(refusal(held, &three).contains("max_inputs"), "the count is bounded");
        let big = vec![NamedInput::new("photo", &[0u8; 17])];
        let big_declared = vec![("photo".to_string(), named_input_hash_v1(&[0u8; 17]))];
        assert!(refusal(big, &big_declared).contains("max_bytes"), "the total is bounded");
    }

    /// X5, restated where a reader of this crate will look for it: a `Derivation` carries bytes,
    /// hashes and names — and no weight, no payment and no exposure. The object's Borsh surface
    /// is destructured so a field that added one would not compile past this test.
    #[test]
    fn a_derivation_carries_no_weight_no_payment_and_no_exposure() {
        let d = derive_named("music/smf/v1", &binding(), ANSWER).unwrap();
        let PalwDerivedArtifactV1 {
            version: _,
            network_domain: _,
            claim_id: _,
            output_root: _,
            grammar_id: _,
            transformer_id: _,
            kind: _,
            dsl_hash: _,
            artifact_hash: _,
            artifact_bytes: _,
            executor_pubkey: _,
        } = &d.object;
    }
}
