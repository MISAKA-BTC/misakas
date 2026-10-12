//! Kind `json` (ADR-0096 Decision 9 — ADR-0078 Decision 8's eighth row): the answer's own JSON
//! text, canonicalized under RFC 8785 (the JSON Canonicalization Scheme, JCS), and an identity
//! transformer over the result. The artifact IS the canonical bytes: `.json`, `application/json`.
//!
//! The row, as the ADR states it:
//!
//! ```text
//!   the DSL        the answer's JSON text
//!   transformer    json/canonical/v1 — RFC 8785 canonicalization, and identity over the result
//!   artifact       .json
//!   determinism    a pure byte function; the artifact IS the canonical bytes
//!   not covered    semantic validation; numbers outside JSON's canonical form; comments and
//!                  trailing commas (refused, not repaired)
//! ```
//!
//! Its id is 28 and not 8: ADR-0078 Decision 9's candidate table had already assigned 8 to
//! `text`, and an id is assigned once and never reused.
//!
//! # The grammar `json/v1`
//!
//! The answer parses as ONE JSON value under RFC 8259, strictly — no comments, no trailing
//! commas, no `NaN` / `Infinity`, nothing after the first value, no byte-order mark, no lone
//! surrogate — and its RFC 8785 bytes are the canonical DSL. Everything else is a refusal by name
//! and never a repair (ADR-0078 Decision 2, X4): a code-fenced answer is a parse failure, not
//! unwrapped; a duplicate member name is refused rather than resolved (RFC 8785 §3.1 requires
//! I-JSON, whose objects have no duplicate names; serde_json would silently keep the last, which
//! is the semantic choice this layer must not make); an answer over [`MAX_DSL_BYTES`] is refused
//! on its byte count before the parser sees it (SA-2), and a value nested past [`MAX_DEPTH`] is
//! refused by depth. A refusal derives nothing, and the inference still certifies and mines.
//!
//! # RFC 8785, and why the crate's own `canon_json` is NOT used here
//!
//! `canon_json` is the canonicalizer the other JSON-shaped grammars share, and it is deliberately
//! not RFC 8785: it refuses every non-integer number (its kinds have integer transformers
//! downstream), keeps a u64 past 2^53 exact, and sorts member names by their UTF-8 bytes. RFC
//! 8785 renders every number in ECMAScript `Number::toString` form (`1.0` is `1`, `1E30` is
//! `1e+30`, `18446744073709551615` is `18446744073709552000` — a JSON number IS an IEEE 754
//! binary64 under I-JSON) and sorts names by UTF-16 code units (an emoji sorts BEFORE U+FB33).
//! The two disagree on exactly those inputs, and the ADR names RFC 8785 so that any JCS
//! implementation in any language reproduces the artifact. So this kind calls
//! `misaka_palw_constraint::canonical::to_rfc8785` — the tree's ONE spelling of the RFC, whose
//! Appendix B vectors are pinned there — and re-spells none of it. The one piece it borrows from
//! `canon_json` is the duplicate-name scan, which is a JSON rule and not a number rule.
//!
//! # The discipline, stated honestly
//!
//! The transformer is byte-exact: it checks that its input is canonical and hands the same bytes
//! back; no arithmetic touches the artifact. The GRAMMAR's number rendering, in the constraint
//! crate, goes through an IEEE 754 binary64: the text is parsed to the nearest double (correctly
//! rounded — the constraint crate enables serde_json's `float_roundtrip` for exactly this) and
//! printed as the shortest digit string that reads back to the same double, laid out the way
//! ECMA-262 §6.1.6.1.20 lays it out. That is a bit-exact function of the double on every IEEE 754
//! host — no addition, no libm, nothing an architecture rounds differently — the RFC's own vectors
//! pin it, and the two-architecture drill (X3) is the empirical check. The manifest's discipline
//! vocabulary has no arm for "canonical JSON, IEEE 754 numbers by the RFC", so the manifest
//! declares [`Discipline::Integer`] — the arm `code` and `map` declare for a byte-exact transform —
//! and this paragraph, the operator doc and the crate's discipline scan (which does not follow
//! calls out of the crate) say where the double lives. Nothing in THIS file spells a
//! floating-point type, and its own test holds it to that.

use crate::canon_json::reject_duplicate_keys;
use crate::{Artifact, DeriveError, Discipline, Grammar, Transformer, TransformerManifest};
use kaspa_consensus_core::palw_derived_v1::kind;
use serde_json::Value;

/// The grammar's name; its id is `H(domain ‖ name)` (`ids::grammar_id_v1`).
pub const GRAMMAR_NAME: &str = "json/v1";
/// The transformer's name.
pub const TRANSFORMER_NAME: &str = "json/canonical/v1";
/// The canonical writer the manifest names: RFC 8785, the JSON Canonicalization Scheme.
pub const WRITER_NAME: &str = "json/rfc8785/jcs-canonical-v1";
/// The artifact's media type and file extension.
pub const MEDIA_TYPE: &str = "application/json";
pub const EXTENSION: &str = "json";

/// **ADR-0078 SA-2's `max_dsl_bytes`, pinned at 1 MiB.** Checked on the byte COUNT before the
/// parser is asked what the bytes spell — a JSON parser is an allocator driven by its input — and
/// checked again on the canonical form, which can be a little LARGER than the answer (`1E30` is
/// four bytes and its canonical `1e+30` is five). Both are "no object" (Decision 2's parse-failure
/// arm). The number is far above any answer a class at the shipped widths emits (the gateway's
/// hard decode cap bounds an answer near 16 KB) and far below what a parser could be made to
/// allocate; it is in the transformer id's preimage, so loosening it is a new transformer.
pub const MAX_DSL_BYTES: u64 = 1 << 20;
/// **The nesting a value may reach, pinned.** A scalar is depth 0, `[]` and `{}` are 1, `[[1]]`
/// is 2. Past this the answer is refused by depth, before the canonicalizer recurses into it.
/// serde_json's own recursion limit (128) is above it, so this number and not the parser's is the
/// one a refusal names.
pub const MAX_DEPTH: usize = 64;
/// The largest artifact: the artifact IS the canonical DSL, so the two ceilings are one number.
pub const MAX_ARTIFACT_BYTES: u64 = MAX_DSL_BYTES;
/// SA-2's step ceiling, in this kind's unit (`canonical-dsl-byte`, the layer's default unit for
/// a kind whose only work is its bytes): the canonical byte length, bounded by the same number.
pub const MAX_STEPS: u64 = MAX_DSL_BYTES;

/// The grammar `json/v1`.
pub struct JsonGrammar;
/// The transformer `json/canonical/v1`: the identity over canonical `json/v1` bytes.
pub struct JsonCanonicalTransformer;

/// This kind's grammar and transformer, as the registry sees them.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![Box::new(JsonGrammar)], vec![Box::new(JsonCanonicalTransformer)])
}

impl Grammar for JsonGrammar {
    fn name(&self) -> &'static str {
        GRAMMAR_NAME
    }

    /// SA-2's wall on the byte count first, then [`canonicalize`]. A refusal anywhere is
    /// `DeriveError::Grammar` (X4).
    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
        crate::check_dsl_bytes(MAX_DSL_BYTES, answer)?;
        canonicalize(answer)
    }
}

impl Transformer for JsonCanonicalTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: TRANSFORMER_NAME,
            kind: kind::JSON,
            grammar: GRAMMAR_NAME,
            // See the module header: the transformer is byte-exact, and the grammar's number
            // rendering is an IEEE 754 double in the constraint crate. There is no arm for that.
            discipline: Discipline::Integer,
            writer: WRITER_NAME,
            source_tree_sha256: crate::SOURCE_TREE_SHA256_HEX,
            // ADR-0078 SA-2: the ceilings this kind enforces, each already a constant above.
            max_dsl_bytes: MAX_DSL_BYTES,
            max_artifact_bytes: MAX_ARTIFACT_BYTES,
            max_steps: MAX_STEPS,
        }
    }

    /// The identity over canonical bytes. Input that is not exactly the grammar's own output —
    /// unparseable, or merely spelled differently (a space, an unsorted member, `1.0` for `1`) —
    /// is refused as `DeriveError::Transformer`, never repaired: a transformer may assume its
    /// input is canonical and must refuse anything else.
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        crate::check_dsl_bytes(MAX_DSL_BYTES, dsl)?;
        let not_canonical = |e: DeriveError| DeriveError::Transformer(format!("input is not canonical json/v1: {e}"));
        let again = canonicalize(dsl).map_err(not_canonical)?;
        if again != dsl {
            return Err(DeriveError::Transformer("input is not canonical json/v1: the bytes differ from their canonical form".into()));
        }
        Ok(Artifact { bytes: dsl.to_vec(), media_type: MEDIA_TYPE, extension: EXTENSION })
    }

    /// The work this DSL asks for is its bytes, and the layer can refuse past `max_steps` before
    /// the run — which it never has to, because the grammar already bounded the canonical form by
    /// the same number. Stated anyway: SA-2 asks for a bound the layer can see.
    fn declared_work(&self, canonical_dsl: &[u8]) -> Option<u64> {
        Some(canonical_dsl.len() as u64)
    }
}

/// **The whole canonicalizer, after SA-2's byte wall.** Parse the answer as one JSON value under
/// RFC 8259 and I-JSON — refusing, never repairing — and emit its RFC 8785 bytes. Every refusal
/// is `DeriveError::Grammar` and names what it saw.
pub fn canonicalize(answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
    let text = std::str::from_utf8(answer).map_err(|_| DeriveError::Grammar("the answer is not UTF-8".into()))?;
    // Named before the parser gets to say "expected value", because a fence is the one wrapper a
    // model reaches for and the one an entrance is most tempted to strip. It is refused, not
    // unwrapped: unwrapping would be changing the answer to make it parse (ADR-0078 Decision 2).
    if text.trim_start().starts_with("```") {
        return Err(DeriveError::Grammar(
            "the answer is wrapped in a code fence; a fence is not JSON and is refused rather than unwrapped (ADR-0078 Decision 2)"
                .into(),
        ));
    }
    // I-JSON (RFC 7493 §2.3), which RFC 8785 §3.1 requires: no duplicate member names. serde_json
    // keeps the last one silently, so the scan runs BEFORE the parser can choose.
    reject_duplicate_keys(text)?;
    let value: Value =
        serde_json::from_str(text).map_err(|e| DeriveError::Grammar(format!("the answer is not one JSON value: {e}")))?;
    let deep = depth(&value);
    if deep > MAX_DEPTH {
        return Err(DeriveError::Grammar(format!("the value nests {deep} deep, past the pinned MAX_DEPTH of {MAX_DEPTH}")));
    }
    let canonical = misaka_palw_constraint::canonical::to_rfc8785(&value)
        .map_err(|why| DeriveError::Grammar(format!("the answer has no canonical form: {why}")))?;
    if canonical.len() as u64 > MAX_DSL_BYTES {
        return Err(DeriveError::Grammar(format!(
            "the canonical form is {} bytes, past the declared max_dsl_bytes of {MAX_DSL_BYTES}; a bound exceeded is no object \
             (ADR-0078 SA-2)",
            canonical.len()
        )));
    }
    Ok(canonical)
}

/// How deep a value nests: a scalar is 0, `[]` and `{}` are 1, `[[1]]` is 2. Recursion is safe
/// here because the parser has already refused anything past its own limit of 128.
pub fn depth(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Value::Object(members) => 1 + members.values().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{ClaimBinding, derive_named, derive_with};
    use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
    use kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN;
    use kaspa_hashes::Hash64;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    /// RFC 8785 §3.2.3's worked example, as the RFC prints it…
    const RFC_EXAMPLE: &str = r#"{
  "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001],
  "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
  "literals": [null, true, false]
}"#;
    /// …and the RFC's own answer, byte for byte.
    const RFC_EXAMPLE_CANONICAL: &str = "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],\"string\":\"€$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}";

    /// **The kind's identity, pinned** (ADR-0078 Decision 8: a kind is versioned by its grammar id
    /// and its transformer id, never by editing a row). Two values, two lifetimes:
    ///
    /// * `grammar_id_v1("json/v1")` is `H(domain ‖ name)` and nothing else — this hex is the
    ///   grammar's name on every chain, forever;
    /// * `transformer_id` covers the manifest, and the manifest carries the build's source-tree
    ///   hash (Decision 3), so the LIVE id moves with every byte under `src/` by design and is
    ///   pinned crate-wide in `tests/transformer_id_pin.rs`. What is forever about the transformer
    ///   is every OTHER field — name, kind 28, grammar, discipline, writer, the three ceilings —
    ///   and that is what the second hex pins: the id of this manifest with the source tree held
    ///   at a fixed reference value. A changed ceiling, writer, kind or discipline fails here by
    ///   name; a comment elsewhere in the crate does not.
    const GRAMMAR_ID_HEX: &str = "eb2c293d392845f5e26d5b5ea804bae4e4a56e5c1690f7da12193331846926e0600768d71f126ab6dd551a64b0130b380d5757cb9a480c1969a23597446668a5";
    const REFERENCE_SOURCE_TREE: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const TRANSFORMER_ID_HEX_UNDER_REFERENCE_TREE: &str = "e09f667a9ba97115f8b0e0e652c114a6de3b93c5754ca5892c7a60287e854cdaf5f74c035e43c60bffa6d6b4f58cb47f4169d0e4fe7a5b687355869391b7313c";

    fn binding() -> ClaimBinding {
        ClaimBinding {
            network_domain: Hash64::from_bytes([1u8; 64]),
            claim_id: Hash64::from_bytes([2u8; 64]),
            output_root: Hash64::from_bytes([3u8; 64]),
            executor_pubkey: vec![7u8; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
        }
    }

    fn canonical(answer: &str) -> Vec<u8> {
        JsonGrammar.canonicalize(answer.as_bytes()).unwrap()
    }

    fn canonical_text(answer: &str) -> String {
        String::from_utf8(canonical(answer)).expect("canonical JSON is UTF-8")
    }

    /// A grammar refusal that mentions `fragment`, returned so a test can look at the rest of it.
    #[track_caller]
    fn refused(answer: &[u8], fragment: &str) -> String {
        match JsonGrammar.canonicalize(answer) {
            Err(DeriveError::Grammar(msg)) => {
                assert!(msg.contains(fragment), "refusal {msg:?} does not mention {fragment:?}");
                msg
            }
            other => panic!("expected a grammar refusal mentioning {fragment:?}, got {other:?}"),
        }
    }

    // ---- (1) the RFC's own words ------------------------------------------------------------

    #[test]
    fn the_rfcs_own_example_canonicalizes_to_the_rfcs_own_answer() {
        assert_eq!(canonical_text(RFC_EXAMPLE), RFC_EXAMPLE_CANONICAL);
    }

    #[test]
    fn member_names_sort_by_utf16_code_units_and_numbers_take_ecmascript_form() {
        // RFC 8785 §3.2.3: the emoji's surrogates (D83D DE02) sort before U+FB33 — the order UTF-8
        // bytes and code points both get wrong, and the reason `canon_json` is not used here.
        assert_eq!(canonical_text(r#"{"דּ":1,"😂":2,"€":3,"1":4}"#), "{\"1\":4,\"€\":3,\"😂\":2,\"\u{fb33}\":1}");
        // Numbers are doubles under I-JSON: the shortest digits that read back, ES layout.
        assert_eq!(
            canonical_text("[1.0, 1E30, 4.50, 2e-3, -0.0, 18446744073709551615, 123456789012345680000]"),
            "[1,1e+30,4.5,0.002,0,18446744073709552000,123456789012345680000]"
        );
        // Strings: exactly the RFC's escapes, everything else raw — including DEL and U+2028.
        assert_eq!(canonical_text(r#""\/\u007f\u2028\u0001""#), "\"/\u{7f}\u{2028}\\u0001\"");
        // The empty forms and a scalar are values too: the grammar is "one JSON value", not "one object".
        for (answer, want) in [("[]", "[]"), ("{}", "{}"), ("\"\"", "\"\""), (" 7 ", "7"), ("null", "null"), ("true", "true")] {
            assert_eq!(canonical_text(answer), want);
        }
    }

    #[test]
    fn canonicalization_is_idempotent_and_the_canonical_form_is_itself_a_legal_answer() {
        let once = canonical(RFC_EXAMPLE);
        let twice = JsonGrammar.canonicalize(&once).unwrap();
        assert_eq!(once, twice);
        // Key order, whitespace and number spelling change nothing …
        let reordered = r#"{ "string" : "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/" , "literals":[null,true,false],
            "numbers":[333333333.3333333, 1e30, 4.5, 0.002, 1e-27] }"#;
        assert_eq!(canonical(reordered), once);
        // … and the derivation of the canonical form is the derivation of the answer.
        let first = derive_named(TRANSFORMER_NAME, &binding(), RFC_EXAMPLE.as_bytes()).unwrap();
        let again = derive_named(TRANSFORMER_NAME, &binding(), &first.canonical_dsl).unwrap();
        assert_eq!(again.object, first.object);
        assert_eq!(first.artifact.bytes, first.canonical_dsl, "the artifact IS the canonical bytes");
    }

    // ---- (2) registration, the manifest, the ids ------------------------------------------

    #[test]
    fn registration_and_manifest() {
        let (grammars, transformers) = register();
        assert_eq!(grammars.len(), 1);
        assert_eq!(transformers.len(), 1);
        assert_eq!(grammars[0].name(), "json/v1");
        let m = transformers[0].manifest();
        assert_eq!(m.name, "json/canonical/v1");
        assert_eq!(m.kind, kind::JSON);
        assert_eq!(m.kind, 28, "8 is `text` in Decision 9's candidate table; an id is never reused");
        assert_eq!(kind::name(m.kind), Some("json"));
        assert_eq!(m.grammar, "json/v1");
        assert_eq!(m.discipline, Discipline::Integer);
        assert_eq!(m.writer, "json/rfc8785/jcs-canonical-v1");
        assert_eq!(m.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX);
        assert_eq!((m.max_dsl_bytes, m.max_artifact_bytes, m.max_steps), (1 << 20, 1 << 20, 1 << 20));
        assert_eq!(m.step_unit(), "canonical-dsl-byte");
        assert!(crate::check_declared_bounds(&m).is_ok());
        assert!(crate::registry::transformer_by_name("json/canonical/v1").is_some());
        assert!(crate::registry::grammar_by_name("json/v1").is_some());
        assert!(crate::registry::transformer_by_id(&transformer_id(&m)).is_some());
        assert!(crate::registry::grammar_by_id(&grammar_id_v1("json/v1")).is_some());
        let a = JsonCanonicalTransformer.run(&canonical("{}")).unwrap();
        assert_eq!((a.media_type, a.extension), ("application/json", "json"));
    }

    #[test]
    fn the_ids_are_the_kinds_identity() {
        let grammar_hex = faster_hex::hex_string(grammar_id_v1(GRAMMAR_NAME).as_byte_slice());
        assert_eq!(grammar_hex, GRAMMAR_ID_HEX, "grammar_id_v1(\"json/v1\") moved — it is the grammar's name forever");
        let live = JsonCanonicalTransformer.manifest();
        let reference = TransformerManifest { source_tree_sha256: REFERENCE_SOURCE_TREE, ..live.clone() };
        let reference_hex = faster_hex::hex_string(transformer_id(&reference).as_byte_slice());
        assert_eq!(
            reference_hex, TRANSFORMER_ID_HEX_UNDER_REFERENCE_TREE,
            "a manifest field other than the source tree moved (name, kind, grammar, discipline, writer or a ceiling): that is a \
             NEW transformer under ADR-0078 Decision 8, not an edit of this one"
        );
        assert_eq!(live.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX, "the live manifest names this build");
        assert_ne!(transformer_id(&live), transformer_id(&reference), "the build is in the live id");
    }

    // ---- (3) every refusal, by name, and never a repair -------------------------------------

    #[test]
    fn refuses_by_name_and_never_repairs() {
        refused(b"```json\n{\"a\":1}\n```", "wrapped in a code fence");
        refused(b"  ```\n[1]\n```", "wrapped in a code fence");
        refused(br#"{"a": 1,}"#, "trailing comma");
        refused(b"[1, 2,]", "trailing comma");
        refused(br#"{"x": NaN}"#, "not one JSON value");
        refused(br#"[Infinity]"#, "not one JSON value");
        refused(br#"[-Infinity]"#, "not one JSON value");
        refused(b"{\"a\": 1} // a comment", "not one JSON value");
        refused(b"/* c */ {\"a\": 1}", "not one JSON value");
        refused(b"{\"a\": 1} {\"b\": 2}", "trailing characters");
        refused(b"{\"a\": 1}\n[2]", "trailing characters");
        refused(b"", "EOF");
        refused(b"   ", "EOF");
        refused(b"{", "EOF");
        refused(b"\xEF\xBB\xBF{}", "not one JSON value");
        refused(b"[1e400]", "out of range");
        refused(br#"["\ud800"]"#, "not one JSON value");
        refused(b"[\"\xff\"]", "not UTF-8");
        refused(br#"{"a": 1, "a": 2}"#, "duplicate key \"a\"");
        refused(br#"{"x": [{"k": 1, "k": 1}]}"#, "duplicate key \"k\"");
        refused(br#"{"a": 1, "a": 2}"#, "duplicate key \"a\"");
        // Nothing above was "fixed": the same answers minus the fault derive.
        assert_eq!(canonical_text(r#"{"a": 1}"#), r#"{"a":1}"#);
        assert_eq!(canonical_text(r#"{"a": {"b": 1}, "c": {"b": 2}}"#), r#"{"a":{"b":1},"c":{"b":2}}"#);
    }

    #[test]
    fn the_byte_wall_and_the_depth_wall() {
        // SA-2's wall, on the byte count, before the parser: names the ceiling and the size.
        let mut over = vec![b' '; MAX_DSL_BYTES as usize + 1];
        over[0] = b'[';
        *over.last_mut().unwrap() = b']';
        let msg = refused(&over, "past the declared max_dsl_bytes of 1048576");
        assert!(msg.contains(&over.len().to_string()), "{msg}");
        // Exactly at the wall is admitted: a bound refuses what is over it and nothing else.
        let mut at = vec![b' '; MAX_DSL_BYTES as usize];
        at[0] = b'[';
        *at.last_mut().unwrap() = b']';
        assert_eq!(JsonGrammar.canonicalize(&at).unwrap(), b"[]");

        // The canonical form can be larger than the answer (`1E30` -> `1e+30`), and the wall
        // applies to it too: 200,000 of them are a 1,000,001-byte answer and a 1,200,001-byte
        // canonical form.
        let mut swell = String::from("[");
        for i in 0..200_000 {
            if i > 0 {
                swell.push(',');
            }
            swell.push_str("1E30");
        }
        swell.push(']');
        assert!(swell.len() as u64 <= MAX_DSL_BYTES, "the answer itself must be under the wall for this to test the second check");
        refused(swell.as_bytes(), "the canonical form is 1200001 bytes, past the declared max_dsl_bytes of 1048576");

        // The depth wall: 64 levels are admitted, 65 are refused by name, and past the parser's
        // own limit the parser refuses first — still a refusal, never a truncation.
        let nested = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert_eq!(depth(&serde_json::from_str::<Value>(&nested(64)).unwrap()), 64);
        assert!(JsonGrammar.canonicalize(nested(MAX_DEPTH).as_bytes()).is_ok());
        refused(nested(MAX_DEPTH + 1).as_bytes(), "nests 65 deep, past the pinned MAX_DEPTH of 64");
        refused(nested(200).as_bytes(), "not one JSON value");
        // depth counts containers, not scalars, and an object counts like an array
        assert_eq!(depth(&serde_json::json!(7)), 0);
        assert_eq!(depth(&serde_json::json!({})), 1);
        assert_eq!(depth(&serde_json::json!({"a": [[1]]})), 3);
    }

    // ---- (4) the transformer is the identity, and refuses what is not canonical -------------

    #[test]
    fn the_transformer_is_the_identity_on_canonical_bytes_and_refuses_anything_else() {
        let c = canonical(RFC_EXAMPLE);
        assert_eq!(JsonCanonicalTransformer.run(&c).unwrap().bytes, c);
        assert_eq!(JsonCanonicalTransformer.declared_work(&c), Some(c.len() as u64));
        // Not canonical, in every way a model or a hand could get there: refused, not repaired.
        let mut padded = c.clone();
        padded.push(b'\n');
        for bad in [padded.as_slice(), b" {}", br#"{"b":1,"a":2}"#, b"1.0", b"1E30", br#"{"a": 1}"#, b"[1,]", b"{", b""] {
            match JsonCanonicalTransformer.run(bad) {
                Err(DeriveError::Transformer(msg)) => assert!(msg.contains("not canonical json/v1"), "{msg}"),
                other => panic!("{bad:?}: expected a transformer refusal, got {other:?}"),
            }
        }
        // …and the same bytes spelled canonically go through.
        assert_eq!(JsonCanonicalTransformer.run(br#"{"a":2,"b":1}"#).unwrap().bytes, br#"{"a":2,"b":1}"#);
        assert_eq!(JsonCanonicalTransformer.run(b"1").unwrap().bytes, b"1");
        assert_eq!(JsonCanonicalTransformer.run(b"1e+30").unwrap().bytes, b"1e+30");
    }

    #[test]
    fn the_layer_sees_the_work_as_bytes_and_the_two_ceilings_are_one_number() {
        let d = derive_named(TRANSFORMER_NAME, &binding(), RFC_EXAMPLE.as_bytes()).unwrap();
        let m = JsonCanonicalTransformer.manifest();
        assert_eq!(JsonCanonicalTransformer.declared_work(&d.canonical_dsl), Some(d.canonical_dsl.len() as u64));
        assert_eq!(d.object.artifact_bytes, d.canonical_dsl.len() as u64);
        assert_eq!(m.max_artifact_bytes, m.max_dsl_bytes, "the artifact is the DSL; one ceiling, spelled twice by the manifest");
        assert_eq!(m.max_steps, m.max_dsl_bytes);
        assert_eq!(d.kind, kind::JSON);
        assert_eq!(d.grammar_id, grammar_id_v1(GRAMMAR_NAME));
        assert!(crate::verify(&d.object, RFC_EXAMPLE.as_bytes()).unwrap().all_match());
    }

    // ---- (5) the fixture corpus --------------------------------------------------------------

    fn corpus_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join("json")
    }

    /// Every `*.json` sample in the corpus, by file name, `golden.json` excluded.
    fn corpus() -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        for entry in std::fs::read_dir(corpus_dir()).expect("corpus/json exists") {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            if name.ends_with(".json") && name != "golden.json" {
                files.insert(name, std::fs::read(&path).unwrap());
            }
        }
        assert!(files.len() >= 8, "the corpus holds {} samples; at least eight are expected", files.len());
        files
    }

    /// Derived samples pin `dsl_hash`, `artifact_hash` and `artifact_bytes`; samples named
    /// `NN-what-refused.json` pin the refusal's words, which is which wall they hit (the drill
    /// compares both, and the name is the house convention scene's `06-inexact-refused.json` set).
    #[test]
    fn corpus_derives_to_the_golden_values() {
        let golden: serde_json::Value = serde_json::from_slice(&std::fs::read(corpus_dir().join("golden.json")).unwrap()).unwrap();
        let golden = golden.as_object().unwrap();
        let files = corpus();
        let grammar_id = grammar_id_v1(GRAMMAR_NAME);
        let (mut derived, mut refusals) = (0usize, 0usize);
        for (name, answer) in &files {
            let g = golden.get(name).unwrap_or_else(|| panic!("{name} has no entry in golden.json; pin it"));
            match derive_with(&JsonGrammar, &JsonCanonicalTransformer, &binding(), answer) {
                Ok(d) => {
                    assert!(!name.contains("-refused"), "{name} is named as a refusal and derived");
                    assert_eq!(g["dsl_hash"].as_str().unwrap(), d.dsl_hash.to_string(), "{name} dsl_hash");
                    assert_eq!(g["artifact_hash"].as_str().unwrap(), d.artifact_hash.to_string(), "{name} artifact_hash");
                    assert_eq!(g["artifact_bytes"].as_u64().unwrap(), d.object.artifact_bytes, "{name} artifact_bytes");
                    assert_eq!(d.object.artifact_bytes as usize, d.artifact.bytes.len());
                    // the ids recomputed directly, the way a consumer does (Decision 5)
                    assert_eq!(dsl_hash_v1(&grammar_id, &d.canonical_dsl), d.dsl_hash);
                    assert_eq!(artifact_hash_v1(&d.artifact.bytes), d.artifact_hash);
                    assert_eq!(d.grammar_id, grammar_id);
                    assert_eq!(d.kind, kind::JSON);
                    // the registry route names the same derivation, and verification agrees
                    let named = derive_named(TRANSFORMER_NAME, &binding(), answer).unwrap();
                    assert_eq!(named.object, d.object);
                    assert!(crate::verify(&d.object, answer).unwrap().all_match(), "{name}");
                    assert!(crate::verify_artifact_bytes(&d.object, &d.artifact.bytes));
                    // a second run is the same bytes, and they are the DSL
                    assert_eq!(JsonCanonicalTransformer.run(&d.canonical_dsl).unwrap().bytes, d.artifact.bytes);
                    assert_eq!(d.artifact.bytes, d.canonical_dsl);
                    // and any JCS implementation reads them back as one value that re-canonicalizes to itself
                    let back: Value = serde_json::from_slice(&d.artifact.bytes).unwrap();
                    assert_eq!(misaka_palw_constraint::canonical::to_rfc8785(&back).unwrap(), d.artifact.bytes);
                    derived += 1;
                }
                Err(e) => {
                    assert!(name.contains("-refused"), "{name}: {e}");
                    assert!(e.is_refusal(), "{name}: {e:?}");
                    assert_eq!(g["refused"].as_str().unwrap(), e.to_string(), "{name}: the refusal moved");
                    refusals += 1;
                }
            }
        }
        for name in golden.keys() {
            assert!(files.contains_key(name), "golden.json names {name}, which is not in the corpus");
        }
        assert!(derived >= 4 && refusals >= 4, "the corpus must exercise both arms: {derived} derived, {refusals} refused");
    }

    /// Re-pin: `cargo test -p misaka-palw-derive print_golden -- --ignored --nocapture`
    /// (the music kind's test of the same name prints its own; filter by module if in doubt).
    #[test]
    #[ignore]
    fn print_golden() {
        let mut out = serde_json::Map::new();
        for (name, answer) in &corpus() {
            let mut entry = serde_json::Map::new();
            match derive_with(&JsonGrammar, &JsonCanonicalTransformer, &binding(), answer) {
                Ok(d) => {
                    entry.insert("dsl_hash".into(), d.dsl_hash.to_string().into());
                    entry.insert("artifact_hash".into(), d.artifact_hash.to_string().into());
                    entry.insert("artifact_bytes".into(), d.object.artifact_bytes.into());
                }
                Err(e) => {
                    entry.insert("refused".into(), e.to_string().into());
                }
            }
            out.insert(name.clone(), entry.into());
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap());
    }

    // ---- (6) the discipline, scanned ---------------------------------------------------------

    /// The crate-wide scan in `lib.rs` covers this file too; this is the per-file copy every kind
    /// carries, token-aware so a pinned hex that happens to contain the digits is not an offender.
    #[test]
    fn no_floating_point_type_is_a_token_of_this_file() {
        let source = include_str!("json.rs");
        let names = [concat!("f", "32"), concat!("f", "64")];
        for (n, line) in source.lines().enumerate() {
            for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                assert!(!names.contains(&token), "json.rs:{}: spells a floating-point type: {line}", n + 1);
            }
        }
    }
}
