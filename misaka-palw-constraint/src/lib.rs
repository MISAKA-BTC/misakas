//! **ADR-0096 Decisions 3 and 7 — the shape an answer may be asked to take, named by its bytes.**
//!
//! Pure functions and nothing else:
//!
//! ```text
//!   schema::parse      a JSON-Schema (draft 2020-12) SUBSET, refused by keyword outside it
//!   schema::validate   an answer against a parsed schema, errors at JSON-pointer paths
//!   canonical          RFC 8785 (JCS) bytes of any JSON value
//!   constraint_id      H("misaka-palw/constraint/v1" ‖ len ‖ canonical bytes)
//!   compile            Part B: a parsed schema → consensus-core's byte-level automaton
//!                      (`PalwDecodeConstraintV1`), content-named by `compile::compiler_id_v1`
//! ```
//!
//! **Why a subset, and why refuse-by-name.** Decision 3 serves `response_format` in two
//! enforcement modes and the mode is the chain's to decide: *committed* once
//! `Params::palw_fp_decode_constraint` is armed (the schema compiles to a decode constraint the
//! seat replays and the court can try), *advisory* on every shipped network today (the schema is
//! rendered into the prompt as text and the answer is validated after the fact). A schema the
//! committed mode could never compile must not be quietly accepted by the advisory one — an
//! integration written against `advisory` would then break the day the fence arms, on the
//! network's schedule rather than the integrator's. So the subset is ONE table, here, and a
//! keyword outside it (`$ref`, `oneOf`, `anyOf`, `allOf`, `not`, `if`, `format`,
//! `patternProperties`, `dependentRequired`, …) is refused by name in both modes rather than
//! approximated.
//!
//! **Why canonical bytes.** The prompt text the model sees, the id the job will carry under Part
//! B, and the digest a consumer checks the answer against are all functions of the SAME bytes —
//! RFC 8785's, which any implementation in any language reproduces. A rendering that depended on
//! the client's key order, or on which serde_json feature a build happened to enable, would put
//! two different prompts under one id.
//!
//! **What Part B changes and what it does not.** Decision 7's `constraint_id` is over the
//! compiled automaton bytes (with a `compiler_id` in their header) and rides inside `fp_job_id_v3`
//! at job version 6. Part A names the constraint by its canonical SCHEMA bytes under the same
//! domain, reports it in `misaka.format.requested.constraint_id`, and commits nothing: an advisory
//! id is advisory. The domain is kept so the two never collide with any other free-prompt id.
//! [`compile`] is the compiler — the same [`constraint_id`] over `compile_v1(schema).to_bytes()`
//! is the id a version-6 job will carry; consensus-core's `constraint_id_v1` is the same function
//! by construction, and `compile::tests` holds the two spellings equal.

pub mod canonical;
pub mod compile;
pub mod schema;

use kaspa_hashes::Hash64;

/// The keyed-hash domain every constraint id is minted under (ADR-0096 Decision 7).
pub const PALW_CONSTRAINT_DOMAIN_V1: &[u8] = b"misaka-palw/constraint/v1";

/// The largest constraint, in canonical bytes, this lane will name (ADR-0096 Decision 7: "bytes
/// are bounded (64 KiB) and refused above it"). Enforced at the entrance, before any inference.
pub const PALW_CONSTRAINT_MAX_BYTES: usize = 64 * 1024;

/// **`H(domain ‖ len_le64 ‖ canonical_bytes)`** — the construction every free-prompt id in
/// consensus-core uses (`canonical_id` in `palw_freeprompt_v3.rs`: a keyed BLAKE2b-512 whose key
/// is the domain, fed the object's length as a little-endian `u64` and then the object). Spelled
/// through the tree's one keyed helper rather than copied, so the id and the transition's own
/// hashing are the same function; the test below re-spells it in consensus-core's two-`update`
/// form and checks they agree.
///
/// The length prefix is what makes the id a function of ONE byte string and not of a
/// concatenation: without it, the same bytes split differently would hash the same.
pub fn constraint_id(canonical_bytes: &[u8]) -> Hash64 {
    let mut preimage = Vec::with_capacity(8 + canonical_bytes.len());
    preimage.extend_from_slice(&(canonical_bytes.len() as u64).to_le_bytes());
    preimage.extend_from_slice(canonical_bytes);
    kaspa_hashes::blake2b_512_keyed(PALW_CONSTRAINT_DOMAIN_V1, &preimage)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The id is consensus-core's `canonical_id` construction under this crate's domain.**
    /// Re-spelled here with the two-`update` form `palw_freeprompt_v3::canonical_id` uses, so a
    /// change to either spelling — the helper's or this crate's — shows up as a mismatch.
    #[test]
    fn the_constraint_id_is_the_keyed_length_prefixed_construction() {
        let bytes = canonical::to_rfc8785(&serde_json::json!({"type": "object", "required": ["a"]})).unwrap();
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_CONSTRAINT_DOMAIN_V1).to_state();
        state.update(&(bytes.len() as u64).to_le_bytes());
        state.update(&bytes);
        let mut expected = [0u8; 64];
        expected.copy_from_slice(state.finalize().as_bytes());
        assert_eq!(constraint_id(&bytes), Hash64::from_bytes(expected));

        // The length prefix is load-bearing: the same bytes under a different length are a
        // different id, and an empty constraint is not the zero hash.
        assert_ne!(constraint_id(&bytes), constraint_id(&bytes[..bytes.len() - 1]));
        assert_ne!(constraint_id(&[]), Hash64::default(), "an empty constraint is still an id, never `none`");
        // The domain is the key, so the same bytes under any other domain differ.
        assert_ne!(constraint_id(&bytes), kaspa_hashes::blake2b_512_keyed(b"misaka-palw/constraint/v0", &bytes));
        assert!(PALW_CONSTRAINT_DOMAIN_V1.len() <= 64, "a BLAKE2b key is at most 64 bytes");
    }

    /// Two schemas that differ only in key order or number spelling are ONE constraint — the id
    /// is over canonical bytes, which is the whole reason the canonical form exists.
    #[test]
    fn the_id_is_over_canonical_bytes_not_over_the_clients_spelling() {
        let a = serde_json::from_str::<serde_json::Value>(r#"{"type":"object","properties":{"n":{"type":"number","maximum":1.0}}}"#)
            .unwrap();
        let b = serde_json::from_str::<serde_json::Value>(r#"{"properties":{"n":{"maximum":1,"type":"number"}},"type":"object"}"#)
            .unwrap();
        let ca = canonical::to_rfc8785(&a).unwrap();
        let cb = canonical::to_rfc8785(&b).unwrap();
        assert_eq!(ca, cb);
        assert_eq!(constraint_id(&ca), constraint_id(&cb));
        assert_eq!(PALW_CONSTRAINT_MAX_BYTES, 65_536);
    }
}
