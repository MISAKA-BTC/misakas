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
        PalwDerivedKeyV1 { claim, transformer: Hash64::from_bytes([0u8; 64]) }
            ..=PalwDerivedKeyV1 { claim, transformer: Hash64::from_bytes([0xFFu8; 64]) }
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
        assert_eq!(kind::ALL[..7].iter().map(|(_, n)| *n).collect::<Vec<_>>(), ["scene", "image", "cad", "code", "map", "music", "simulation"]);
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

    #[test]
    fn a_claims_rows_are_one_contiguous_range() {
        let range = PalwDerivedKeyV1::claim_range(h(0x02));
        assert!(range.contains(&PalwDerivedKeyV1 { claim: h(0x02), transformer: h(0x00) }));
        assert!(range.contains(&PalwDerivedKeyV1 { claim: h(0x02), transformer: h(0xFF) }));
        assert!(!range.contains(&PalwDerivedKeyV1 { claim: h(0x03), transformer: h(0x00) }));
        assert!(!range.contains(&PalwDerivedKeyV1 { claim: h(0x01), transformer: h(0xFF) }));
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
