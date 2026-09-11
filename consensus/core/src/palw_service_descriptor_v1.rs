//! **ADR-0101 — a membership is proven by the chain and served by anyone: the signed service
//! descriptor, and the check a client runs against chain facts.**
//!
//! ADR-0095 made a Position a membership: a line declares what each tier grants, the chain
//! computes a holder's tier at the tip, and a gateway reads that tier to decide how to serve. What
//! it did not say is WHO serves — and the answer this repository commits to is **anyone**: the
//! line controls the PRODUCT (what a tier grants, declared on chain), providers control the
//! SERVING, and the chain proves the MEMBERSHIP. A membership whose only server is the line's own
//! machine dies with that machine; one any provider can serve does not.
//!
//! This module is what makes "any provider" something a client can check instead of trust:
//!
//! * [`palw_benefit_server_v1`] — which grant needs whom: THE CHAIN (the holder mark — the fold
//!   writes it), ANY PROVIDER holding bytes under a root the chain names (priority, quota, the
//!   modes a version runs, a preview or an early version — a client checks the root), or THE
//!   LINE'S ORIGIN (its developer room and its support desk — relations with the line's own
//!   people, which a stranger's server cannot provide).
//! * [`PalwServiceDescriptorV1`] — a provider's signed statement: which line, which grants,
//!   which artifact roots it serves, where, from when until when. Signed under
//!   [`PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT`] by the provider's own key.
//! * [`palw_service_descriptor_check_v1`] — the client's check against what the chain says about
//!   the line: a descriptor may only offer grants the line declared, only serve roots the chain
//!   names for the line, and only claim an origin grant under a key of one of the line's own
//!   bonds; an expired one, or one not yet valid, is refused by name.
//!
//! **Nothing here is consensus.** No rule verifies a descriptor, no fold reads one, the chain holds
//! no URL (ADR-0067 Decision 6; ADR-0095 §8), and the signing context is in no committed set on
//! purpose — a descriptor is a statement to a client. Serving is not adjudicated: a provider that
//! does not serve loses its clients, never a bond (ADR-0095 §8's "a bond behind the promise"
//! stays rejected).
//!
//! **A Position never moves here, and never pays** (ADR-0101 Decision 5, settled). A descriptor has
//! no price field in Positions, no escrow, no holder address to credit; a provider READS a holding
//! through the proof a holder signs (ADR-0095's reference check) and receives nothing from it.

use crate::Hash64;
use crate::palw_model_benefits_v1::grant;

pub const PALW_SERVICE_DESCRIPTOR_VERSION_V1: u16 = 1;
pub const PALW_SERVICE_DESCRIPTOR_DOMAIN_ID_V1: &[u8] = b"misaka-palw/service-descriptor/id/v1";
/// The provider's ML-DSA-87 context. In no committed set: no consensus rule verifies it.
pub const PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/service-descriptor/mldsa87/v1";
/// Every keyed domain this family uses, for the cross-family uniqueness sweep.
pub const PALW_SERVICE_DESCRIPTOR_ALL_DOMAINS: &[&[u8]] =
    &[PALW_SERVICE_DESCRIPTOR_DOMAIN_ID_V1, PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT];
/// Bounds on what one descriptor may carry — a statement, not a catalogue.
pub const PALW_SERVICE_DESCRIPTOR_MAX_ROOTS: usize = 64;
pub const PALW_SERVICE_DESCRIPTOR_MAX_ENDPOINTS: usize = 8;
pub const PALW_SERVICE_DESCRIPTOR_MAX_ENDPOINT_BYTES: usize = 256;

/// Who can provide a grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBenefitServerV1 {
    /// The fold provides it; no server is involved (the holder mark, ADR-0095 §4.9).
    TheChain,
    /// Any provider holding bytes under a root the chain names for the line — the class's
    /// artifact or a version's. A client checks the root; nobody's permission is involved.
    AnyProviderByRoot,
    /// Only the line's own people: a key of the line's owner, developer or maintainer bond.
    LineOrigin,
}

/// **Which grant needs whom.** One spelling for every client and every provider; a bit this build
/// does not know needs nobody it can name, and is refused by the check rather than guessed.
pub fn palw_benefit_server_v1(bit: u32) -> Option<PalwBenefitServerV1> {
    match bit {
        grant::HOLDER_VOICE => Some(PalwBenefitServerV1::TheChain),
        grant::EARLY_VERSION | grant::PRIVATE_BETA | grant::PRIORITY_INFERENCE | grant::EXPERIMENTAL | grant::INFERENCE_QUOTA => {
            Some(PalwBenefitServerV1::AnyProviderByRoot)
        }
        grant::DEVELOPER_ACCESS | grant::SUPPORT => Some(PalwBenefitServerV1::LineOrigin),
        _ => None,
    }
}

/// **A provider's signed statement of service.** Borsh for its id; JSON for its transport.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwServiceDescriptorV1 {
    pub version: u16,
    /// The line whose holders this provider serves.
    pub line_id: Hash64,
    /// The grants it serves — a subset of what the line declared.
    pub grants: u32,
    /// The artifact roots it serves under this line's name — each one the chain names for the line.
    pub roots: Vec<Hash64>,
    /// Where to reach it. Transport is not the chain's business; the strings are the provider's.
    pub endpoints: Vec<String>,
    /// The DAA window the statement holds for.
    pub valid_from_daa: u64,
    pub expires_daa: u64,
    /// The provider's ML-DSA-87 public key, and its signature over [`palw_service_descriptor_id_v1`].
    pub provider_pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

/// The id a provider signs: the network domain and every field but the signature, borsh, keyed.
pub fn palw_service_descriptor_id_v1(network_domain: Hash64, d: &PalwServiceDescriptorV1) -> Hash64 {
    let mut unsigned = d.clone();
    unsigned.signature.clear();
    let bytes = borsh::to_vec(&unsigned).expect("a descriptor is borsh-serializable");
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_SERVICE_DESCRIPTOR_DOMAIN_ID_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(&bytes);
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// What the chain says about a line, as a client read it (RPC: the line, its versions, its
/// benefits row, the bonds its roles name).
#[derive(Clone, Debug, Default)]
pub struct PalwLineServiceFactsV1 {
    pub line_id: Hash64,
    /// The union of every tier's grants in the line's benefits row IN EFFECT at `now_daa`
    /// (`palw_model_benefits_in_effect_v1`): a lapsed promise offers nothing to serve.
    pub declared_grants: u32,
    /// The class's registered artifact root and every version root the line published.
    pub roots: Vec<Hash64>,
    /// The registered public keys of the line's owner, developer and maintainer bonds.
    pub origin_pubkeys: Vec<Vec<u8>>,
    pub now_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwServiceDescriptorError {
    #[error("descriptor version {got}; this build reads version {expected}")]
    Version { got: u16, expected: u16 },
    #[error("the descriptor is for line {got}, not {expected}")]
    OtherLine { got: Hash64, expected: Hash64 },
    #[error("the descriptor offers no grant")]
    NoGrants,
    #[error("grant bits {0:#x} are not ones this build knows")]
    UnknownGrants(u32),
    #[error("grants {0:?} are not ones the line declares in effect now")]
    NotDeclared(Vec<&'static str>),
    #[error("grants {0:?} are the chain's to provide, not a server's")]
    TheChainsGrant(Vec<&'static str>),
    #[error("grants {0:?} are the line's origin's, and the provider's key is not one of the line's bonds")]
    OriginGrantByAStranger(Vec<&'static str>),
    #[error("a grant served by root needs a root, and the descriptor names none")]
    NoRoots,
    #[error("root {0} is not one the chain names for this line")]
    ForeignRoot(Hash64),
    #[error("the descriptor is bounded: {0}")]
    Bounds(&'static str),
    #[error("the descriptor holds from DAA {from} to {until}; now is {now}")]
    OutsideItsWindow { from: u64, until: u64, now: u64 },
    #[error("the signature does not verify under the provider's key")]
    Signature,
}

/// What a checked descriptor is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwServiceProviderKindV1 {
    /// A provider the line's own bonds sign for.
    Origin,
    /// Anyone else — serving only what a root proves.
    Open,
}

/// **The client's check.** Shape and bounds, then the window, then the grants against what the
/// line declares in effect, then the roots against what the chain names, then the origin grants
/// against the line's own keys, then the signature (`verify` is the caller's ML-DSA-87 verifier —
/// `kaspa_txscript::verify_mldsa87_with_context` in every binary this tree ships).
pub fn palw_service_descriptor_check_v1(
    network_domain: Hash64,
    d: &PalwServiceDescriptorV1,
    facts: &PalwLineServiceFactsV1,
    verify: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<PalwServiceProviderKindV1, PalwServiceDescriptorError> {
    if d.version != PALW_SERVICE_DESCRIPTOR_VERSION_V1 {
        return Err(PalwServiceDescriptorError::Version { got: d.version, expected: PALW_SERVICE_DESCRIPTOR_VERSION_V1 });
    }
    if d.line_id != facts.line_id {
        return Err(PalwServiceDescriptorError::OtherLine { got: d.line_id, expected: facts.line_id });
    }
    if d.roots.len() > PALW_SERVICE_DESCRIPTOR_MAX_ROOTS {
        return Err(PalwServiceDescriptorError::Bounds("more roots than one descriptor carries"));
    }
    if d.endpoints.len() > PALW_SERVICE_DESCRIPTOR_MAX_ENDPOINTS
        || d.endpoints.iter().any(|e| e.is_empty() || e.len() > PALW_SERVICE_DESCRIPTOR_MAX_ENDPOINT_BYTES)
    {
        return Err(PalwServiceDescriptorError::Bounds("an endpoint list past its bounds, or an empty endpoint"));
    }
    if d.valid_from_daa > d.expires_daa || facts.now_daa < d.valid_from_daa || facts.now_daa >= d.expires_daa {
        return Err(PalwServiceDescriptorError::OutsideItsWindow { from: d.valid_from_daa, until: d.expires_daa, now: facts.now_daa });
    }
    if d.grants == 0 {
        return Err(PalwServiceDescriptorError::NoGrants);
    }
    if d.grants & !grant::KNOWN != 0 {
        return Err(PalwServiceDescriptorError::UnknownGrants(d.grants & !grant::KNOWN));
    }
    let undeclared = d.grants & !facts.declared_grants;
    if undeclared != 0 {
        return Err(PalwServiceDescriptorError::NotDeclared(grant::names_of(undeclared)));
    }
    let bits = || (0..32).map(|i| 1u32 << i).filter(|b| d.grants & b != 0);
    let of = |server: PalwBenefitServerV1| bits().filter(|b| palw_benefit_server_v1(*b) == Some(server)).fold(0u32, |a, b| a | b);
    let chains = of(PalwBenefitServerV1::TheChain);
    if chains != 0 {
        return Err(PalwServiceDescriptorError::TheChainsGrant(grant::names_of(chains)));
    }
    if of(PalwBenefitServerV1::AnyProviderByRoot) != 0 && d.roots.is_empty() {
        return Err(PalwServiceDescriptorError::NoRoots);
    }
    if let Some(foreign) = d.roots.iter().find(|r| !facts.roots.contains(r)) {
        return Err(PalwServiceDescriptorError::ForeignRoot(*foreign));
    }
    let is_origin = facts.origin_pubkeys.contains(&d.provider_pubkey);
    let origin_grants = of(PalwBenefitServerV1::LineOrigin);
    if origin_grants != 0 && !is_origin {
        return Err(PalwServiceDescriptorError::OriginGrantByAStranger(grant::names_of(origin_grants)));
    }
    let id = palw_service_descriptor_id_v1(network_domain, d);
    if !verify(&d.provider_pubkey, id.as_byte_slice(), &d.signature, PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT) {
        return Err(PalwServiceDescriptorError::Signature);
    }
    Ok(if is_origin { PalwServiceProviderKindV1::Origin } else { PalwServiceProviderKindV1::Open })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain() -> Hash64 {
        Hash64::from_u64_word(0xD0)
    }

    /// A verifier that accepts exactly `sig == b"ok:" ‖ pubkey` over the right context — the
    /// shape of the rule without a key; the integration test signs with a real ML-DSA-87 key.
    fn verify(pk: &[u8], _msg: &[u8], sig: &[u8], ctx: &[u8]) -> bool {
        ctx == PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT && sig.strip_prefix(b"ok:") == Some(pk)
    }

    fn facts() -> PalwLineServiceFactsV1 {
        PalwLineServiceFactsV1 {
            line_id: Hash64::from_u64_word(7),
            declared_grants: grant::PRIORITY_INFERENCE | grant::INFERENCE_QUOTA | grant::SUPPORT | grant::HOLDER_VOICE,
            roots: vec![Hash64::from_u64_word(100), Hash64::from_u64_word(101)],
            origin_pubkeys: vec![b"owner".to_vec()],
            now_daa: 5_000,
        }
    }

    fn descriptor(pk: &[u8], grants: u32) -> PalwServiceDescriptorV1 {
        PalwServiceDescriptorV1 {
            version: 1,
            line_id: Hash64::from_u64_word(7),
            grants,
            roots: vec![Hash64::from_u64_word(100)],
            endpoints: vec!["https://provider.example/v1".into()],
            valid_from_daa: 4_000,
            expires_daa: 9_000,
            provider_pubkey: pk.to_vec(),
            signature: [b"ok:".as_slice(), pk].concat(),
        }
    }

    #[test]
    fn every_known_grant_names_who_serves_it_and_an_unknown_bit_names_nobody() {
        for bit in (0..32).map(|i| 1u32 << i) {
            assert_eq!(palw_benefit_server_v1(bit).is_some(), bit & grant::KNOWN != 0, "bit {bit:#x}");
        }
        assert_eq!(palw_benefit_server_v1(grant::PRIORITY_INFERENCE), Some(PalwBenefitServerV1::AnyProviderByRoot));
        assert_eq!(palw_benefit_server_v1(grant::SUPPORT), Some(PalwBenefitServerV1::LineOrigin));
        assert_eq!(palw_benefit_server_v1(grant::HOLDER_VOICE), Some(PalwBenefitServerV1::TheChain));
    }

    #[test]
    fn a_stranger_serves_by_root_and_the_origin_serves_its_own() {
        let f = facts();
        let open = descriptor(b"stranger", grant::PRIORITY_INFERENCE | grant::INFERENCE_QUOTA);
        assert_eq!(palw_service_descriptor_check_v1(domain(), &open, &f, verify), Ok(PalwServiceProviderKindV1::Open));
        let origin = descriptor(b"owner", grant::PRIORITY_INFERENCE | grant::SUPPORT);
        assert_eq!(palw_service_descriptor_check_v1(domain(), &origin, &f, verify), Ok(PalwServiceProviderKindV1::Origin));
    }

    #[test]
    fn the_check_refuses_by_name() {
        let f = facts();
        let check = |d: PalwServiceDescriptorV1| palw_service_descriptor_check_v1(domain(), &d, &f, verify).unwrap_err();
        assert!(
            matches!(check(descriptor(b"stranger", grant::SUPPORT)), PalwServiceDescriptorError::OriginGrantByAStranger(n) if n == vec!["SUPPORT"])
        );
        assert!(
            matches!(check(descriptor(b"stranger", grant::PRIVATE_BETA)), PalwServiceDescriptorError::NotDeclared(n) if n == vec!["PRIVATE_BETA"])
        );
        assert!(matches!(check(descriptor(b"owner", grant::HOLDER_VOICE)), PalwServiceDescriptorError::TheChainsGrant(_)));
        assert_eq!(check(descriptor(b"stranger", 1 << 20)), PalwServiceDescriptorError::UnknownGrants(1 << 20));
        assert_eq!(check(descriptor(b"stranger", 0)), PalwServiceDescriptorError::NoGrants);
        assert_eq!(
            check(PalwServiceDescriptorV1 {
                roots: vec![Hash64::from_u64_word(999)],
                ..descriptor(b"stranger", grant::PRIORITY_INFERENCE)
            }),
            PalwServiceDescriptorError::ForeignRoot(Hash64::from_u64_word(999))
        );
        assert_eq!(
            check(PalwServiceDescriptorV1 { roots: vec![], ..descriptor(b"stranger", grant::PRIORITY_INFERENCE) }),
            PalwServiceDescriptorError::NoRoots
        );
        assert!(matches!(
            check(PalwServiceDescriptorV1 { expires_daa: 5_000, ..descriptor(b"stranger", grant::PRIORITY_INFERENCE) }),
            PalwServiceDescriptorError::OutsideItsWindow { .. }
        ));
        assert!(matches!(
            check(PalwServiceDescriptorV1 { line_id: Hash64::from_u64_word(8), ..descriptor(b"stranger", grant::PRIORITY_INFERENCE) }),
            PalwServiceDescriptorError::OtherLine { .. }
        ));
        assert_eq!(
            check(PalwServiceDescriptorV1 {
                signature: b"ok:someone-else".to_vec(),
                ..descriptor(b"stranger", grant::PRIORITY_INFERENCE)
            }),
            PalwServiceDescriptorError::Signature
        );
        assert!(matches!(
            check(PalwServiceDescriptorV1 { endpoints: vec![String::new()], ..descriptor(b"stranger", grant::PRIORITY_INFERENCE) }),
            PalwServiceDescriptorError::Bounds(_)
        ));
    }

    #[test]
    fn the_id_binds_the_network_and_every_field_but_the_signature() {
        let d = descriptor(b"stranger", grant::PRIORITY_INFERENCE);
        let id = palw_service_descriptor_id_v1(domain(), &d);
        assert_eq!(palw_service_descriptor_id_v1(domain(), &PalwServiceDescriptorV1 { signature: vec![1], ..d.clone() }), id);
        assert_ne!(palw_service_descriptor_id_v1(Hash64::from_u64_word(0xD1), &d), id, "another network");
        for (name, other) in [
            ("grants", PalwServiceDescriptorV1 { grants: grant::INFERENCE_QUOTA, ..d.clone() }),
            ("roots", PalwServiceDescriptorV1 { roots: vec![Hash64::from_u64_word(101)], ..d.clone() }),
            ("endpoints", PalwServiceDescriptorV1 { endpoints: vec!["https://elsewhere.example".into()], ..d.clone() }),
            ("expiry", PalwServiceDescriptorV1 { expires_daa: 9_001, ..d.clone() }),
            ("key", PalwServiceDescriptorV1 { provider_pubkey: b"other".to_vec(), ..d.clone() }),
        ] {
            assert_ne!(palw_service_descriptor_id_v1(domain(), &other), id, "{name} is inside the id");
        }
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<PalwServiceDescriptorV1>(&json).unwrap(), d, "the transport form round-trips");
    }
}
