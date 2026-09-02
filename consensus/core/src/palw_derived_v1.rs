//! **ADR-0078: what was made from it is committed; the thing itself never rides.**
//!
//! A certified free-prompt claim commits the model's output ids (`output_root`). What a person
//! keeps is usually one step further on: the answer rendered under a registered grammar into a
//! canonical DSL, and a deterministic transformer's artifact of that DSL — a mesh, an image, a
//! MIDI file, a map, a trace. The chain holds the DERIVATION of that thing and never the thing:
//! one compact object per (claim, transformer), stored beside the claim, retired with it
//! (ADR-0078 Decisions 1 and 4).
//!
//! What this module is: the object, its total-binding id, the ids of the names it carries
//! (grammar, transformer, DSL, artifact), the kind table, the bounds the transition applies, and
//! the state row the chain keeps. What it is NOT: a transformer. The chain never runs one
//! (Decision 5) — verification belongs to whoever holds the answer, and every step of it is a pure
//! function of bytes the consumer has and ids the chain has (`misaka-palw-derive` is that
//! consumer's crate).
//!
//! Weight, payment, exposure: none (Decision 4, invariant X5). The object is a statement priced by
//! its transaction fee, authorised by the claim's executor (the bond's key — the same comparison
//! `FreePromptCommitted` makes), and checkable by anyone.
//!
//! The two names it carries — `grammar_id` and `transformer_id` — are opaque to every line here:
//! consensus resolves neither, and [`check_derived_shape_v1`]'s doc says why that is SA-5 held
//! rather than SA-5 skipped.

use crate::palw_state_v2::PalwStateV2Error;
use blake2b_simd::Params;
use kaspa_hashes::Hash64;

/// The object's wire version. A field addition is a new version, never an in-place edit.
pub const PALW_DERIVED_V1_VERSION: u16 = 1;

/// **ADR-0078 Decision 4: the most derivations one claim may carry.** One inference, one claim,
/// zero or more derivations of it — a scene AND its music, say — but a bounded number, because
/// the table is state and a claim is a finite thing. Counted off the state table by the
/// transition; the fifth is refused and the block stands.
pub const PALW_DERIVED_MAX_PER_CLAIM: usize = 4;

/// The ML-DSA-87 public key length the executor key must have (ADR-0019): a derived object
/// carries the executor's key so the chain can compare it with the bond's, and a key of any
/// other length compares with nothing.
pub const PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN: usize = 2592;

/// **The executor signature's length, pinned — which is what makes Decision 1 a rule rather than
/// an intention** (audit 2026-09-02, X1).
///
/// The ride list used to ask only that a signature be PRESENT, and that left the one object whose
/// whole purpose is "the thing never rides" as the only lifecycle object with a free-length byte
/// field. The consequence is not theoretical: a failing object is DROPPED and the carrying block
/// STANDS (`palw_v2_accepted_objects`), so a derivation whose `signature` held a GLB would be
/// refused at acceptance while its megabytes sat in an accepted transaction forever — DSL or
/// artifact bytes as consensus carriage, exactly what Decision 1 says the chain never accepts "in
/// any chunking, under any size".
///
/// With this pinned every field of the carriage is fixed-width — 7 × `Hash64`, two `u16`, one
/// `u64`, a 2592-byte key and a 4627-byte signature — so a derivation's wire size is a CONSTANT
/// and there is nowhere left to put a byte. That is the sentence a future ADR has to argue
/// against. Same value and same reason as every other PALW signature length check
/// ([`crate::dns_finality::STAKE_ATTESTATION_SIG_LEN`]).
pub const PALW_DERIVED_V1_SIGNATURE_LEN: usize = crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;

pub const PALW_DERIVED_V1_DOMAIN_ID: &[u8] = b"misaka-palw/derived-v1/derived-id/v1";
/// What the executor signs: the object's own id under its own domain, so a signature over a
/// derivation can never be read as a signature over a commitment, a spend or a registration.
pub const PALW_DERIVED_V1_DOMAIN_MESSAGE: &[u8] = b"misaka-palw/derived-v1/message/v1";
/// ML-DSA-87 signing context (one context per family, audit P0-6).
pub const PALW_DERIVED_V1_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/derived-v1/mldsa87/v1";
/// `grammar_id = H(name)` — a grammar is named by its registered name (Decision 2).
pub const PALW_DERIVED_V1_DOMAIN_GRAMMAR_ID: &[u8] = b"misaka-palw/derived-v1/grammar-id/v1";
/// `transformer_id = H(manifest)` — a transformer is named by its manifest's canonical bytes
/// (Decision 3).
pub const PALW_DERIVED_V1_DOMAIN_TRANSFORMER_ID: &[u8] = b"misaka-palw/derived-v1/transformer-id/v1";
/// `dsl_hash = H(grammar_id ‖ canonical bytes)` (Decision 2).
pub const PALW_DERIVED_V1_DOMAIN_DSL_HASH: &[u8] = b"misaka-palw/derived-v1/dsl-hash/v1";
/// `artifact_hash = H(artifact bytes)` (Decision 4).
pub const PALW_DERIVED_V1_DOMAIN_ARTIFACT_HASH: &[u8] = b"misaka-palw/derived-v1/artifact-hash/v1";

/// Every domain this module keys, so the cross-family uniqueness sweep can see them.
pub const PALW_DERIVED_V1_ALL_DOMAINS: &[&[u8]] = &[
    PALW_DERIVED_V1_DOMAIN_ID,
    PALW_DERIVED_V1_DOMAIN_MESSAGE,
    PALW_DERIVED_V1_MLDSA87_CONTEXT,
    PALW_DERIVED_V1_DOMAIN_GRAMMAR_ID,
    PALW_DERIVED_V1_DOMAIN_TRANSFORMER_ID,
    PALW_DERIVED_V1_DOMAIN_DSL_HASH,
    PALW_DERIVED_V1_DOMAIN_ARTIFACT_HASH,
];

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn canonical_id(domain: &[u8], bytes: &[u8]) -> Hash64 {
    let mut state = keyed(domain);
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(bytes);
    finish(state)
}

/// **ADR-0078 Decision 4: the derivation.** Every field is in the id's preimage (total binding);
/// the signature rides beside it on the consensus object, never inside it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDerivedArtifactV1 {
    pub version: u16,
    pub network_domain: Hash64,
    /// The free-prompt claim whose output this derives from.
    pub claim_id: Hash64,
    /// MUST equal the claim's committed `output_root` — a cross-check, not a second source.
    pub output_root: Hash64,
    pub grammar_id: Hash64,
    pub transformer_id: Hash64,
    /// The kind table's id (`kind`); the chain checks `kind != 0` and interprets nothing else
    /// (Decision 9).
    pub kind: u16,
    pub dsl_hash: Hash64,
    pub artifact_hash: Hash64,
    pub artifact_bytes: u64,
    /// MUST equal the claim's executor bond key.
    pub executor_pubkey: Vec<u8>,
}

/// `derived_id_v1 = H(canonical(object))` — total binding, every field in the preimage.
pub fn derived_id_v1(object: &PalwDerivedArtifactV1) -> Hash64 {
    let bytes = borsh::to_vec(object).expect("PalwDerivedArtifactV1 is borsh-serializable");
    canonical_id(PALW_DERIVED_V1_DOMAIN_ID, &bytes)
}

/// What the executor signs: the id, under the message domain. The id already binds the network
/// domain, the claim, the executor key and every name, so nothing signed here can be re-used for
/// another derivation, another claim or another chain.
pub fn palw_derived_message_v1(object: &PalwDerivedArtifactV1) -> Hash64 {
    let mut state = keyed(PALW_DERIVED_V1_DOMAIN_MESSAGE);
    state.update(derived_id_v1(object).as_byte_slice());
    finish(state)
}

/// A grammar's id, from its registered name (e.g. `scene/v1`).
pub fn grammar_id_v1(name: &str) -> Hash64 {
    canonical_id(PALW_DERIVED_V1_DOMAIN_GRAMMAR_ID, name.as_bytes())
}

/// A transformer's id, from its manifest's canonical bytes.
pub fn transformer_id_v1(manifest_canonical_bytes: &[u8]) -> Hash64 {
    canonical_id(PALW_DERIVED_V1_DOMAIN_TRANSFORMER_ID, manifest_canonical_bytes)
}

/// `H(grammar_id ‖ canonical DSL bytes)` (Decision 2).
pub fn dsl_hash_v1(grammar_id: &Hash64, canonical_dsl: &[u8]) -> Hash64 {
    let mut state = keyed(PALW_DERIVED_V1_DOMAIN_DSL_HASH);
    state.update(grammar_id.as_byte_slice());
    state.update(&(canonical_dsl.len() as u64).to_le_bytes());
    state.update(canonical_dsl);
    finish(state)
}

/// `H(artifact bytes)` (Decision 4).
pub fn artifact_hash_v1(artifact: &[u8]) -> Hash64 {
    canonical_id(PALW_DERIVED_V1_DOMAIN_ARTIFACT_HASH, artifact)
}

/// The stateless shape checks — what the ride list and the transition both apply before any
/// state is read. A refusal names its reason.
///
/// # What SA-5 is, for consensus and for the consumer
///
/// ADR-0078 SA-5 reads "a derivation is refused for a `kind` whose transformer manifest is not
/// published in the tree at the object's `transformer_id`", and the sentence has two readers.
///
/// **For consensus it is not a rule, and it cannot become one here.** "Published in the tree" is a
/// question about a build's transformer registry, and the registry is
/// `misaka_palw_derive::registry::transformer_by_id` — a crate that depends on THIS one and never
/// the other way, because the chain runs no transformer (Decision 5). A node holds no
/// `transformer_id → manifest` table and there is no consensus object that would install one, so
/// the transition cannot tell an unpublished transformer from a published one it has not heard of.
/// Worse, if it could, it would break the ADR's own Decision 9 one section earlier: "the chain
/// checks `kind != 0` and interprets nothing else… adding a row is therefore never a ruleset
/// move" (X8). A chain that refused an unknown `transformer_id` would make shipping a transformer
/// — or a kind — a consensus change, and two builds with different registries would disagree about
/// which blocks' objects are valid. So what consensus checks about the names a derivation carries
/// is exactly this: `kind != 0`, and `(claim_id, transformer_id)` is a table key it has not seen.
/// Both `grammar_id` and `transformer_id` are opaque 64-byte names to every line of this crate.
///
/// **For the consumer and the submitter it is a rule, and it is enforced where the registry is.**
/// `misaka-palw-derive`'s producer path resolves the transformer by name before it builds an
/// object at all, and its verifier refuses an object whose `transformer_id` resolves to nothing
/// (`DeriveError::UnknownTransformer`) rather than reporting a pass — which is SA-5's actual
/// promise: an unverifiable statement is one nobody should have submitted, and one every reader
/// can SAY is unverifiable. On chain it is still a statement, and Decision 5 already prices that:
/// what a derivation nobody can check costs its executor is its executor's name on it.
///
/// The one-sentence form, so the code and the ADR cannot drift: **SA-5 binds the submitter and the
/// consumer; for consensus a derivation is shape, uniqueness, caps and the claim binding, and the
/// chain resolves neither `transformer_id` nor `kind`.**
pub fn check_derived_shape_v1(object: &PalwDerivedArtifactV1) -> Result<(), &'static str> {
    if object.version != PALW_DERIVED_V1_VERSION {
        return Err("a derived artifact of another version cannot be read by this ruleset");
    }
    if object.kind == 0 {
        return Err("kind 0 is reserved; a derivation names the kind a person asked for (ADR-0078 Decision 9)");
    }
    if object.executor_pubkey.len() != PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN {
        return Err("the executor key is not an ML-DSA-87 public key, so it can be compared with no bond's");
    }
    if object.artifact_bytes == 0 {
        return Err("an artifact of zero bytes is not a thing anyone keeps; nothing was derived");
    }
    Ok(())
}

/// The shape checks PLUS the one that bounds the carriage: the signature is an ML-DSA-87
/// signature and nothing else (ADR-0078 Decision 1, invariant X1).
///
/// Separate from [`check_derived_shape_v1`] because the signature rides BESIDE the object rather
/// than inside it — the id is total over the object, and a signature inside its own preimage is
/// not a thing anyone can produce. Both the ride list (a block rule, in
/// `tx_validation_in_isolation`) and the transition call this, for the reason the module beside
/// them states about bond retirement: one lock is a lock somebody removes while refactoring.
pub fn check_derived_carriage_v1(object: &PalwDerivedArtifactV1, signature: &[u8]) -> Result<(), &'static str> {
    check_derived_shape_v1(object)?;
    if signature.len() != PALW_DERIVED_V1_SIGNATURE_LEN {
        return Err(
            "a derived artifact's signature is not an ML-DSA-87 signature — the one object that must never carry bytes may not carry a free-length field",
        );
    }
    Ok(())
}

/// The key of the state table: `(claim, transformer)` — unique per Decision 4, ordered by claim
/// first so a claim's rows are one contiguous range (the retirement sweep reads it that way).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDerivedKeyV1 {
    pub claim: Hash64,
    pub transformer: Hash64,
}

impl PalwDerivedKeyV1 {
    /// The inclusive range of every key under `claim`.
    pub fn claim_range(claim: Hash64) -> std::ops::RangeInclusive<PalwDerivedKeyV1> {
        PalwDerivedKeyV1 { claim, transformer: Hash64::from_bytes([0u8; 64]) }..=PalwDerivedKeyV1 {
            claim,
            transformer: Hash64::from_bytes([0xFFu8; 64]),
        }
    }
}

/// What the chain keeps beside the claim (Decision 4): the names, the hashes, the size, the
/// object's id and when it was accepted. Not the executor key (the claim's bond names it) and not
/// the claim id or the transformer id (the key does).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDerivedRowV1 {
    pub derived_id: Hash64,
    pub grammar_id: Hash64,
    pub kind: u16,
    pub dsl_hash: Hash64,
    pub artifact_hash: Hash64,
    pub artifact_bytes: u64,
    pub accepted_daa: u64,
}

impl PalwDerivedRowV1 {
    pub fn from_object(object: &PalwDerivedArtifactV1, accepted_daa: u64) -> Self {
        Self {
            derived_id: derived_id_v1(object),
            grammar_id: object.grammar_id,
            kind: object.kind,
            dsl_hash: object.dsl_hash,
            artifact_hash: object.artifact_hash,
            artifact_bytes: object.artifact_bytes,
            accepted_daa,
        }
    }
}

/// **The kind table (ADR-0078 Decisions 8 and 9).** One id per thing a person asks for; ids are
/// assigned once and never reused; the chain interprets none of them beyond `kind != 0`. A row
/// whose transformer does not exist yet still has its id, so nothing renumbers later.
pub mod kind {
    // Decision 8 — v1, transformers in the tree.
    pub const SCENE: u16 = 1;
    pub const IMAGE: u16 = 2;
    pub const CAD: u16 = 3;
    pub const CODE: u16 = 4;
    pub const MAP: u16 = 5;
    pub const MUSIC: u16 = 6;
    pub const SIMULATION: u16 = 7;
    // Decision 9 — the candidate table, ids fixed now.
    pub const TEXT: u16 = 8;
    pub const DESIGN: u16 = 9;
    pub const GAME: u16 = 10;
    pub const CIRCUIT: u16 = 11;
    pub const STORYBOARD: u16 = 12;
    pub const VOICE: u16 = 13;
    pub const ANIMATION: u16 = 14;
    pub const UI: u16 = 15;
    pub const DATA: u16 = 16;
    pub const SCIENCE: u16 = 17;
    pub const ROBOT: u16 = 18;
    pub const AGENT: u16 = 19;
    pub const DATABASE: u16 = 20;
    pub const ZK: u16 = 21;
    pub const CONTRACT: u16 = 22;
    pub const MATH: u16 = 23;
    pub const MOLECULE: u16 = 24;
    pub const MANUFACTURING: u16 = 25;
    pub const BUILDING: u16 = 26;
    pub const PROCEDURAL: u16 = 27;

    /// Every assigned id with its name, in id order.
    pub const ALL: &[(u16, &str)] = &[
        (SCENE, "scene"),
        (IMAGE, "image"),
        (CAD, "cad"),
        (CODE, "code"),
        (MAP, "map"),
        (MUSIC, "music"),
        (SIMULATION, "simulation"),
        (TEXT, "text"),
        (DESIGN, "design"),
        (GAME, "game"),
        (CIRCUIT, "circuit"),
        (STORYBOARD, "storyboard"),
        (VOICE, "voice"),
        (ANIMATION, "animation"),
        (UI, "ui"),
        (DATA, "data"),
        (SCIENCE, "science"),
        (ROBOT, "robot"),
        (AGENT, "agent"),
        (DATABASE, "database"),
        (ZK, "zk"),
        (CONTRACT, "contract"),
        (MATH, "math"),
        (MOLECULE, "molecule"),
        (MANUFACTURING, "manufacturing"),
        (BUILDING, "building"),
        (PROCEDURAL, "procedural"),
    ];

    /// The table's name for an id, `None` for an id the table has not assigned. A reader's
    /// convenience only: the chain accepts any non-zero kind (Decision 9).
    pub fn name(kind: u16) -> Option<&'static str> {
        ALL.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n)
    }

    /// The id for a name, `None` for a name the table does not have.
    pub fn id(name: &str) -> Option<u16> {
        ALL.iter().find(|(_, n)| *n == name).map(|(k, _)| *k)
    }
}

/// The transition's refusals, named (ADR-0078 X2). Kept as a conversion into the state error so
/// the arm reads as the list in Decision 4.
pub fn derived_refusal(reason: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::DerivedShapeRefused(reason)
}

// ---------------------------------------------------------------------------------------------
// ADR-0078 Decision 6: the DSL under a data-availability election — the retention payload
// ---------------------------------------------------------------------------------------------

pub const PALW_FP_DSL_V1_MAGIC: [u8; 4] = *b"FPD1";

/// The most DSL bytes one payload carries. The DSL is the model's answer in canonical form —
/// kilobytes for a scene, at most the widest context a class admits rendered as text — and the
/// transport's material cap is 16 MiB; this stays well under both.
pub const PALW_FP_DSL_V1_MAX_BYTES: usize = 4 << 20;

/// **A claim's canonical DSL, served on request when its executor elected to** (ADR-0078
/// Decision 6). Off by default — the DSL is the answer to a person's prompt, and ADR-0044
/// Decision 8's sentence about silently publishing prompts applies to answers word for word.
/// The payload names the derivation it belongs to, so a reader holding the chain's row checks
/// `dsl_hash` against it before believing the bytes; the payload's own check is that the bytes
/// hash to the `dsl_hash` it declares.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpDslV1 {
    pub claim_id: Hash64,
    pub derived_id: Hash64,
    pub grammar_id: Hash64,
    pub dsl_hash: Hash64,
    pub dsl: Vec<u8>,
}

/// Encode with the magic prefix. The inverse of [`palw_fp_dsl_decode_v1`].
pub fn palw_fp_dsl_encode_v1(claim_id: Hash64, derived_id: Hash64, grammar_id: Hash64, dsl: &[u8]) -> Vec<u8> {
    let body =
        borsh::to_vec(&PalwFpDslV1 { claim_id, derived_id, grammar_id, dsl_hash: dsl_hash_v1(&grammar_id, dsl), dsl: dsl.to_vec() })
            .expect("a DSL payload serializes");
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&PALW_FP_DSL_V1_MAGIC);
    out.extend_from_slice(&body);
    out
}

/// `None` for foreign magic, junk, an oversize DSL, or bytes that do not hash to the `dsl_hash`
/// the payload declares. Whether that `dsl_hash` is the one the CHAIN holds for `(claim,
/// derived_id)` is the reader's comparison, against the state row.
pub fn palw_fp_dsl_decode_v1(bytes: &[u8]) -> Option<PalwFpDslV1> {
    let body = bytes.strip_prefix(&PALW_FP_DSL_V1_MAGIC)?;
    let payload: PalwFpDslV1 = borsh::from_slice(body).ok()?;
    if payload.dsl.is_empty() || payload.dsl.len() > PALW_FP_DSL_V1_MAX_BYTES {
        return None;
    }
    if dsl_hash_v1(&payload.grammar_id, &payload.dsl) != payload.dsl_hash {
        return None;
    }
    Some(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    pub(crate) fn sample() -> PalwDerivedArtifactV1 {
        PalwDerivedArtifactV1 {
            version: PALW_DERIVED_V1_VERSION,
            network_domain: h(0x01),
            claim_id: h(0x02),
            output_root: h(0x03),
            grammar_id: grammar_id_v1("scene/v1"),
            transformer_id: transformer_id_v1(b"manifest"),
            kind: kind::SCENE,
            dsl_hash: h(0x06),
            artifact_hash: h(0x07),
            artifact_bytes: 12_345,
            executor_pubkey: vec![0x11; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
        }
    }

    /// **Golden vectors (ADR-0078 Q-01).** Any change to the object's Borsh shape, a domain, or
    /// the id derivation moves one of these; update them only with a new object version.
    #[test]
    fn golden_vector_ids_are_frozen() {
        let o = sample();
        let actual = [
            ("derived_id", derived_id_v1(&o).to_string()),
            ("message", palw_derived_message_v1(&o).to_string()),
            ("grammar_id", grammar_id_v1("scene/v1").to_string()),
            ("transformer_id", transformer_id_v1(b"manifest").to_string()),
            ("dsl_hash", dsl_hash_v1(&h(0x05), b"{}").to_string()),
            ("artifact_hash", artifact_hash_v1(b"glb").to_string()),
        ];
        let frozen = [
            ("derived_id", "16ed50f5929284d587fae8c652145e6e"),
            ("message", "e3911309054835315f6e0baddb50e7df"),
            ("grammar_id", "d59ca0a0da3c2fa46ad5cb99aa352d51"),
            ("transformer_id", "c4f157f407d1cd36d4a6b7bb43d20f3c"),
            ("dsl_hash", "379e039e7e2d7528ae33df089daf1906"),
            ("artifact_hash", "edd490651da6af2da2d1359c5e906b68"),
        ];
        let mismatches: Vec<String> = actual
            .iter()
            .zip(frozen.iter())
            .filter(|((_, got), (_, want))| &got[..32] != *want)
            .map(|((name, got), _)| format!("{name}: {}", &got[..32]))
            .collect();
        assert!(mismatches.is_empty(), "golden vectors moved: {mismatches:?}");
    }

    /// **Per-field mutation moves the id** (ADR-0078 Q-01): every field is in the preimage, and
    /// no two single-field mutations collide.
    #[test]
    fn every_field_moves_the_derived_id() {
        let base = sample();
        let base_id = derived_id_v1(&base);
        type Mutate = (&'static str, Box<dyn Fn(&mut PalwDerivedArtifactV1)>);
        let mutations: Vec<Mutate> = vec![
            ("version", Box::new(|o| o.version += 1)),
            ("network_domain", Box::new(|o| o.network_domain = h(0xA1))),
            ("claim_id", Box::new(|o| o.claim_id = h(0xA2))),
            ("output_root", Box::new(|o| o.output_root = h(0xA3))),
            ("grammar_id", Box::new(|o| o.grammar_id = h(0xA4))),
            ("transformer_id", Box::new(|o| o.transformer_id = h(0xA5))),
            ("kind", Box::new(|o| o.kind = kind::MUSIC)),
            ("dsl_hash", Box::new(|o| o.dsl_hash = h(0xA6))),
            ("artifact_hash", Box::new(|o| o.artifact_hash = h(0xA7))),
            ("artifact_bytes", Box::new(|o| o.artifact_bytes += 1)),
            ("executor_pubkey", Box::new(|o| o.executor_pubkey[0] ^= 1)),
        ];
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(base_id);
        for (name, mutate) in mutations {
            let mut m = base.clone();
            mutate(&mut m);
            let id = derived_id_v1(&m);
            assert_ne!(id, base_id, "{name} must move the id");
            assert!(seen.insert(id), "{name}'s mutation collides with another");
            assert_ne!(palw_derived_message_v1(&m), palw_derived_message_v1(&base), "{name} must move the message");
        }
        // The Borsh field count is the object's whole surface; a field added without joining the
        // list above is caught here.
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
        } = &base;
    }

    /// **ADR-0079 Decision 2 and S1: `DerivedArtifactV1` carries no security field, and the
    /// enforcement is this classification.**
    ///
    /// Decision 2 names this struct by name — "`execution_commitment_v3`, the attempt envelope,
    /// `PalwFreePromptCommitmentV3`, the claim, the certification objects and `DerivedArtifactV1`
    /// gain no security field, no policy id, no attestation, and no confinement flag" — and says
    /// the enforcement is "the exhaustive field-classification test from ADR-0072 Decision 8".
    /// That test exists for the attempt envelope (`every_priced_field_is_pinned_or_is_the
    /// _challenge`) and had no counterpart here, so the one struct ADR-0079 added to the list was
    /// the one struct with nothing refusing to compile.
    ///
    /// The categories differ from D8's because a derivation is not priced bytes: it wins no
    /// lottery, moves no target and earns no weight (X5, proved byte for byte by
    /// `four_derivations_leave_every_priced_table_byte_identical`), so D8's own worry — "a field
    /// the producer chooses freely and no rule pins is a nonce by another name" — has no grinding
    /// surface to land on. What replaces it is the question S1 actually asks: is every field
    /// either something the chain compares against its own state, something the shape refuses, or
    /// a NAME a consumer recomputes from bytes it holds? A `security_policy_hash` is none of the
    /// three — it is unfalsifiable, since a host that ran wide open commits whichever value it
    /// likes — and adding one does not compile, because the destructure below is exhaustive.
    #[test]
    fn every_field_is_a_chain_equality_a_shape_bound_or_a_name_the_consumer_recomputes() {
        #[derive(Debug, PartialEq, Eq)]
        enum Pin {
            /// An equality against chain state at acceptance: the ruleset, the network, the
            /// claim, the bond. A wrong value is refused by name.
            ChainEquality,
            /// A stateless bound the shape check applies; the chain reads no meaning into it.
            ShapeOnly,
            /// A content name. The chain stores it and resolves nothing; whoever holds the bytes
            /// recomputes it and gets the object's value or a demonstrable mismatch (X6).
            ConsumerRecomputes,
        }
        let o = sample();
        // Exhaustive: a field added tomorrow does not compile until it is classified below.
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
        } = o.clone();
        let classified = [
            ("version", Pin::ChainEquality),             // == PALW_DERIVED_V1_VERSION
            ("network_domain", Pin::ChainEquality),      // == this network's domain (acceptance)
            ("claim_id", Pin::ChainEquality),            // a FreePrompt claim this chain holds
            ("output_root", Pin::ChainEquality),         // == the claim's committed output_root
            ("grammar_id", Pin::ConsumerRecomputes),     // H(the grammar's registered name)
            ("transformer_id", Pin::ConsumerRecomputes), // H(manifest); also half the table key
            ("kind", Pin::ShapeOnly),                    // != 0, and the chain interprets no kind (X8)
            ("dsl_hash", Pin::ConsumerRecomputes),       // H(grammar_id ‖ canonical DSL bytes)
            ("artifact_hash", Pin::ConsumerRecomputes),  // H(artifact bytes)
            ("artifact_bytes", Pin::ConsumerRecomputes), // the artifact's length, != 0 by shape
            ("executor_pubkey", Pin::ChainEquality),     // == the claim's bond key, and it signed
        ];
        assert_eq!(classified.len(), 11, "one row per field of the struct destructured above");
        // No fourth category exists, which is the point: a policy hash, an attestation or a
        // confinement flag is not a chain equality (nothing to compare it with), not a shape bound
        // (any value is well-formed) and not a name a consumer recomputes (the host that chose it
        // is the only witness). ADR-0079 S1.
        for (name, _) in &classified {
            assert!(
                !name.contains("policy") && !name.contains("attest") && !name.contains("sandbox") && !name.contains("confine"),
                "ADR-0079 Decision 2: {name} reads like a security field, and none may enter this struct"
            );
        }
        // The chain equalities are equalities against values the chain already holds, so none of
        // them is a value the executor may choose: two of them are constants of the ruleset.
        assert_eq!(o.version, PALW_DERIVED_V1_VERSION);
        assert_eq!(o.executor_pubkey.len(), PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN);
        // …and the names really are recomputable functions rather than free 64-byte fields: each
        // is a pure hash of bytes the consumer holds, and each moves when those bytes move.
        assert_eq!(grammar_id_v1("scene/v1"), o.grammar_id);
        assert_ne!(grammar_id_v1("scene/v2"), o.grammar_id);
        assert_eq!(transformer_id_v1(b"manifest"), o.transformer_id);
        assert_ne!(transformer_id_v1(b"manifesu"), o.transformer_id);
        assert_ne!(dsl_hash_v1(&o.grammar_id, b"{}"), dsl_hash_v1(&o.grammar_id, b"{ }"));
        assert_ne!(artifact_hash_v1(b"glb"), artifact_hash_v1(b"glc"));
    }

    /// **X8 and SA-5: the chain interprets no kind and resolves no transformer.**
    ///
    /// Decision 9 fixes every kind id "whether or not its transformer exists yet" so that "adding
    /// a row is therefore never a ruleset move", and SA-5's "not published in the tree" is a
    /// question about a build's transformer registry that this crate cannot ask — the registry
    /// lives in `misaka-palw-derive`, which depends on this crate and not the reverse.
    ///
    /// Both make the same demand of the shape check, and it is a demand to NOT check something:
    /// an unassigned kind and an unheard-of `transformer_id` must pass. The failure this pins is a
    /// plausible future edit — `kind::name(k).ok_or(...)`, or a registry lookup — which would turn
    /// shipping a transformer into a ruleset move and make two builds with different registries
    /// disagree about which objects a block may carry.
    #[test]
    fn an_unassigned_kind_and_an_unknown_transformer_pass_the_shape_check() {
        let mut o = sample();
        // Past the last assigned row, and at the top of the space.
        for k in [28u16, 1_000, u16::MAX] {
            o.kind = k;
            assert_eq!(kind::name(k), None, "{k} is deliberately an id the table has not assigned");
            assert!(check_derived_shape_v1(&o).is_ok(), "kind {k} must ride: adding a row is never a ruleset move (X8)");
        }
        // A transformer id that names nothing, and a grammar id that names nothing: opaque names,
        // stored and never resolved (SA-5 for consensus).
        let mut o = sample();
        o.transformer_id = h(0xEE);
        o.grammar_id = h(0xEF);
        assert!(check_derived_shape_v1(&o).is_ok(), "consensus resolves neither name — the consumer does, and says so when it cannot");
        assert!(check_derived_carriage_v1(&o, &vec![1; PALW_DERIVED_V1_SIGNATURE_LEN]).is_ok());
    }

    #[test]
    fn the_message_is_not_the_id() {
        let o = sample();
        assert_ne!(palw_derived_message_v1(&o), derived_id_v1(&o));
    }

    #[test]
    fn shape_checks_name_their_refusals() {
        assert!(check_derived_shape_v1(&sample()).is_ok());
        let mut o = sample();
        o.version = 2;
        assert!(check_derived_shape_v1(&o).unwrap_err().contains("version"));
        let mut o = sample();
        o.kind = 0;
        assert!(check_derived_shape_v1(&o).unwrap_err().contains("kind 0"));
        let mut o = sample();
        o.executor_pubkey.pop();
        assert!(check_derived_shape_v1(&o).unwrap_err().contains("ML-DSA-87"));
        let mut o = sample();
        o.artifact_bytes = 0;
        assert!(check_derived_shape_v1(&o).unwrap_err().contains("zero bytes"));
    }

    #[test]
    fn kind_table_ids_and_names_are_unique_and_never_zero() {
        let mut ids = std::collections::BTreeSet::new();
        let mut names = std::collections::BTreeSet::new();
        for (id, name) in kind::ALL {
            assert_ne!(*id, 0);
            assert!(ids.insert(*id), "kind id {id} assigned twice");
            assert!(names.insert(*name), "kind name {name} assigned twice");
            assert_eq!(kind::name(*id), Some(*name));
            assert_eq!(kind::id(name), Some(*id));
        }
        // Decision 8's seven, in the table's order, with the ids the ADR fixes.
        assert_eq!(
            kind::ALL[..7].iter().map(|(_, n)| *n).collect::<Vec<_>>(),
            ["scene", "image", "cad", "code", "map", "music", "simulation"]
        );
        assert_eq!(kind::name(0), None);
        assert_eq!(kind::name(28), None);
    }

    #[test]
    fn domains_are_unique_within_the_module() {
        for (i, a) in PALW_DERIVED_V1_ALL_DOMAINS.iter().enumerate() {
            for b in PALW_DERIVED_V1_ALL_DOMAINS.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    /// **…and against every other PALW family, which is what the constant's doc promised.**
    ///
    /// `PALW_DERIVED_V1_ALL_DOMAINS` says it exists "so the cross-family uniqueness sweep can see
    /// them", and no sweep did: every other family in this tree carries this test (see
    /// `palw_reference`, `palw_carriage`, `palw_legs`, `palw_bisect`), and the derivation family
    /// landed without one. A shared domain is a signature or an id from one family readable as
    /// another's, which is the whole reason each family keys its own hashes.
    #[test]
    fn derived_domains_are_unique_across_every_palw_family() {
        let mut foreign: std::collections::BTreeSet<&[u8]> = std::collections::BTreeSet::new();
        for list in [
            crate::palw_attempt_v2::PALW_ATTEMPT_V2_ALL_DOMAINS,
            crate::palw_bisect::PALW_BISECT_ALL_DOMAINS,
            crate::palw_block_commitment::PALW_BLOCK_COMMITMENT_ALL_DOMAINS,
            crate::palw_carriage::PALW_CARRIAGE_ALL_DOMAINS,
            crate::palw_catalog_coverage::PALW_COVERAGE_ALL_DOMAINS,
            crate::palw_court_v2::PALW_COURT_V2_ALL_DOMAINS,
            crate::palw_e2e_adjudicability::PALW_E2E_ALL_DOMAINS,
            crate::palw_freeprompt_v3::PALW_FP_V3_ALL_DOMAINS,
            crate::palw_job_identity::PALW_JOB_ALL_DOMAINS,
            crate::palw_job_panel::PALW_PANEL_ALL_DOMAINS,
            crate::palw_legs::PALW_LEGS_ALL_DOMAINS,
            crate::palw_mode_v2::PALW_MODE_V2_ALL_DOMAINS,
            crate::palw_panel_v2::PALW_PANEL_V2_ALL_DOMAINS,
            crate::palw_receipt::PALW_RECEIPT_ALL_DOMAINS,
            crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS,
            crate::palw_registry::PALW_REGISTRY_ALL_DOMAINS,
            crate::palw_routing::PALW_ROUTING_ALL_DOMAINS,
            crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS,
            crate::palw_slash::PALW_S_ALL_DOMAINS,
            crate::palw_state_v2::PALW_STATE_V2_ALL_DOMAINS,
            crate::palw_step::PALW_STEP_ALL_DOMAINS,
            crate::palw_step_leg::PALW_STEP_LEG_ALL_DOMAINS,
            crate::palw_v2::PALW_V2_ALL_DOMAINS,
        ] {
            foreign.extend(list.iter().copied());
        }
        for d in PALW_DERIVED_V1_ALL_DOMAINS {
            assert!(d.len() <= 64, "blake2b key cap exceeded: {}", String::from_utf8_lossy(d));
            assert!(!foreign.contains(*d), "the derivation family reuses a foreign domain: {}", String::from_utf8_lossy(d));
        }
    }

    /// **The signing context is part of the ruleset's identity** (audit 2026-09-02).
    ///
    /// `PALW_V2_SIGNATURE_CONTEXTS`'s own doc is "every ML-DSA context the ConsensusV2 acceptance
    /// layer verifies under", and audit M2-23 wrote down what an omission costs: two builds share
    /// a ruleset id while disagreeing about what a signature authorises, and — because a refused
    /// object is dropped and the block stands — they diverge with no block ever rejected and
    /// nothing in either log saying so. The acceptance layer verifies a derivation under this
    /// context (`palw_v2_validate_objects`), so it belongs in the committed list.
    ///
    /// **RED ON PURPOSE as of this commit.** The list lives in `palw_mode_v2.rs`, which this
    /// stream does not own, so the finding ships as a failing assertion and the one-line fix is
    /// the integrator's. It moves `signature_contexts_root`, hence `palw_ruleset_id_v2`, hence
    /// every preset's fingerprint — which is exactly why it is a re-pin and not a lane edit.
    #[test]
    fn the_derived_signing_context_is_inside_the_ruleset_id() {
        assert!(
            crate::palw_mode_v2::PALW_V2_SIGNATURE_CONTEXTS.contains(&PALW_DERIVED_V1_MLDSA87_CONTEXT),
            "the acceptance layer verifies derivations under a context the ruleset id does not commit to. \
             FIX (palw_mode_v2.rs, not this stream's file): add \
             `crate::palw_derived_v1::PALW_DERIVED_V1_MLDSA87_CONTEXT,` to PALW_V2_SIGNATURE_CONTEXTS and to the \
             membership list in `the_ruleset_id_covers_every_component_decision_11_names`; it moves signature_contexts_root, \
             so the ruleset id and every preset fingerprint move with it"
        );
    }

    /// **X1: there is nowhere in the carriage to put a byte.**
    ///
    /// Decision 1 says the chain never accepts the DSL bytes or the artifact bytes "in any
    /// chunking, under any size", and this is the executable form of that sentence: every field
    /// of the object is fixed-width, the key is pinned to ML-DSA-87's length, the signature
    /// beside it is pinned to ML-DSA-87's — so the whole carriage is a constant number of bytes,
    /// whatever the artifact weighed. A future ADR that wants the chain to hold a thing has to
    /// move one of these numbers, which is exactly the argument Decision 1 asks it to make.
    #[test]
    fn the_carriage_is_a_constant_size_whatever_the_artifact_weighed() {
        let mut small = sample();
        small.artifact_bytes = 1;
        let mut huge = sample();
        huge.artifact_bytes = u64::MAX;
        let encoded = |o: &PalwDerivedArtifactV1| borsh::to_vec(o).unwrap().len();
        assert_eq!(encoded(&small), encoded(&huge), "the artifact's SIZE is a number; its bytes are not here");
        // 7 × Hash64, two u16, one u64, the length-prefixed key: no variable-length field left.
        assert_eq!(encoded(&small), 7 * 64 + 2 + 2 + 8 + 4 + PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN);
        assert!(check_derived_carriage_v1(&small, &vec![1; PALW_DERIVED_V1_SIGNATURE_LEN]).is_ok());
        for len in [0usize, 1, PALW_DERIVED_V1_SIGNATURE_LEN - 1, PALW_DERIVED_V1_SIGNATURE_LEN + 1, 4 << 20] {
            let err = check_derived_carriage_v1(&small, &vec![1; len]).expect_err("only an ML-DSA-87 signature rides beside it");
            assert!(err.contains("free-length field"), "{len}: {err}");
        }
    }

    /// **X1, the other half: the DSL payload is a delivery format, not a consensus object.**
    /// `PalwFpDslV1` is what an executor serves under Decision 6's election — it is not a
    /// `PalwConsensusObjectV2` variant, so no ride list, no chunk group and no transition arm can
    /// name it, and the only callers in the tree are the gateway and the CLI.
    #[test]
    fn the_dsl_payload_is_not_a_consensus_object() {
        let bytes = palw_fp_dsl_encode_v1(h(0x02), h(0x09), grammar_id_v1("scene/v1"), br#"{"v":1}"#);
        assert!(borsh::from_slice::<crate::palw_state_v2::PalwConsensusObjectV2>(&bytes).is_err(), "a DSL payload is not an object");
        assert!(
            borsh::from_slice::<crate::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2>(&bytes).is_err(),
            "and it is not lifecycle carriage"
        );
    }

    /// **The derivation was APPENDED to the object enum, and the tags prove it** (ADR-0078 X7 —
    /// ADR-0077's R1: the seat's bytes are what they were).
    ///
    /// Borsh tags an enum by POSITION. A variant inserted anywhere but the end silently re-labels
    /// every object kind on the wire — a `CourtClosed` decoding as a `CourtDisclosed` — and
    /// nothing else in the tree pins these. The comment on the variant said "appended last"; this
    /// is the assertion.
    #[test]
    fn the_derivation_was_appended_so_no_earlier_object_tag_moved() {
        use crate::palw_state_v2::PalwConsensusObjectV2 as O;
        let tag = |o: &O| borsh::to_vec(o).unwrap()[0];
        let retire = O::BondRetireRequested {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::default(), 0)),
            signature: vec![1; 4],
        };
        let licensed = O::ReceiptLicensed { claim: h(0x02), receipts: Vec::new() };
        let chunk = O::ObjectChunk { group: h(0x03), index: 0, count: 1, bytes: vec![1] };
        let derived = O::DerivedArtifactV1 { object: Box::new(sample()), signature: vec![1; PALW_DERIVED_V1_SIGNATURE_LEN] };
        assert_eq!(tag(&retire), 2, "BondRetireRequested's tag moved");
        assert_eq!(tag(&licensed), 6, "ReceiptLicensed's tag moved");
        assert_eq!(tag(&chunk), 15, "ObjectChunk's tag moved");
        assert_eq!(tag(&derived), 16, "the derivation must be the LAST variant, or every tag above shifted");
    }

    #[test]
    fn a_claims_rows_are_one_contiguous_range() {
        let range = PalwDerivedKeyV1::claim_range(h(0x02));
        assert!(range.contains(&PalwDerivedKeyV1 { claim: h(0x02), transformer: h(0x00) }));
        assert!(range.contains(&PalwDerivedKeyV1 { claim: h(0x02), transformer: h(0xFF) }));
        assert!(!range.contains(&PalwDerivedKeyV1 { claim: h(0x03), transformer: h(0x00) }));
        assert!(!range.contains(&PalwDerivedKeyV1 { claim: h(0x01), transformer: h(0xFF) }));
    }

    #[test]
    fn a_dsl_payload_round_trips_and_refuses_a_tampered_body() {
        let grammar = grammar_id_v1("scene/v1");
        let bytes = palw_fp_dsl_encode_v1(h(0x02), h(0x09), grammar, br#"{"v":1}"#);
        assert!(bytes.starts_with(&PALW_FP_DSL_V1_MAGIC));
        let decoded = palw_fp_dsl_decode_v1(&bytes).expect("decodes");
        assert_eq!(decoded.claim_id, h(0x02));
        assert_eq!(decoded.dsl, br#"{"v":1}"#);
        assert_eq!(decoded.dsl_hash, dsl_hash_v1(&grammar, br#"{"v":1}"#));
        // Flip a DSL byte: the declared hash no longer matches.
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(palw_fp_dsl_decode_v1(&tampered).is_none());
        assert!(palw_fp_dsl_decode_v1(b"FPM1junk").is_none());
        assert!(palw_fp_dsl_decode_v1(&palw_fp_dsl_encode_v1(h(1), h(2), grammar, b"")).is_none(), "an empty DSL is no DSL");
    }

    #[test]
    fn the_row_keeps_what_the_key_does_not() {
        let o = sample();
        let row = PalwDerivedRowV1::from_object(&o, 77);
        assert_eq!(row.derived_id, derived_id_v1(&o));
        assert_eq!(row.kind, kind::SCENE);
        assert_eq!(row.artifact_bytes, 12_345);
        assert_eq!(row.accepted_daa, 77);
    }
}
