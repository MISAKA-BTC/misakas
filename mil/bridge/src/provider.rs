//! **Seam 2 — bonded provider identity.**
//!
//! Until now the bridge trusted a `provider_id` string. That is fine for a dev harness and
//! useless against an adversary: anyone could claim any id, and "independence" (submitter ≠
//! replica) was enforced against a self-declared label. This module replaces the label with the
//! chain's own identity and the repo's universal authentication pattern.
//!
//! **Identity.** A provider is its BOND OUTPOINT (`txid:index`) — there is no `ProviderId` type
//! in the registry, and status is never stored, always derived. The credential is
//! `owner_pubkey_hash = validator_id_from_pubkey(owner_public_key)`.
//!
//! **Authentication.** The repo's pattern, copied clause for clause from
//! `palw_provider_unbond_request_authorized`: the message carries `owner_public_key` + an
//! ML-DSA-87 signature over a domain-separated signing hash; we check (1) the key hashes to the
//! registry's `owner_pubkey_hash`, (2) the bond is ACTIVE at the point of view, (3) the
//! signature verifies under the object's context. Verification uses the consensus verifier
//! (`kaspa_txscript::verify_mldsa87_with_context`, the deterministic portable path).
//!
//! **Hot keys.** Owner keys are cold. A desktop provider therefore delegates to a session key
//! with [`PalwProviderSessionAuthorizationV1`] — the node's own off-chain delegation object,
//! verified with the node's own verifier. Registration presents the authorization once; each
//! later request is signed by the session key.
//!
//! What this does NOT do: it cannot make a bond, unbond it, or slash it — those are chain
//! actions. It verifies, and it produces evidence (see `arbitration.rs`).

use std::collections::BTreeMap;

use kaspa_consensus_core::dns_finality::validator_id_from_pubkey;
use kaspa_consensus_core::palw::PalwProviderBondRecord;
use kaspa_consensus_core::palw::da::PalwProviderSessionAuthorizationV1;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use serde::{Deserialize, Serialize};

use crate::chain::{BondFacts, ChainFacts, format_outpoint, parse_hash64, record_status_agrees};
use crate::match_key::{decode_hex, hash64_hex};

/// ML-DSA-87 context for request signatures made by a provider's SESSION key against this
/// bridge. Bridge-local and disjoint from every consensus context in `signature_domains.rs`, so
/// a bridge request signature can never be replayed as a consensus object signature (and vice
/// versa — FIPS-204 binds the context into the signature).
pub const BRIDGE_REQUEST_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-bridge-v1/request/mldsa87";
/// Keyed-BLAKE2b domain for the request signing hash.
pub const BRIDGE_REQUEST_SIGNING_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/request-signing";

pub const MLDSA87_PK_LEN: usize = 2592;
pub const MLDSA87_SIG_LEN: usize = 4627;

/// A provider registered with this bridge: chain identity + the hot key that signs its traffic.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisteredProvider {
    pub bond_outpoint: String,
    pub owner_public_key_hex: String,
    pub credential_hex: String,
    pub session_public_key_hex: String,
    pub session_valid_from_epoch: u64,
    pub session_valid_until_epoch: u64,
    /// The chain facts observed at registration (re-checked before every consequential use).
    pub bond: BondFacts,
}

impl RegisteredProvider {
    pub fn credential(&self) -> Result<Hash64, String> {
        parse_hash64(&self.credential_hex)
    }
    pub fn outpoint(&self) -> Result<TransactionOutpoint, String> {
        crate::chain::parse_outpoint(&self.bond_outpoint)
    }
    pub fn record(&self) -> Result<PalwProviderBondRecord, String> {
        self.bond.to_record(decode_hex(&self.owner_public_key_hex)?)
    }
    pub fn session_valid_at(&self, epoch: u64) -> bool {
        epoch >= self.session_valid_from_epoch && epoch <= self.session_valid_until_epoch
    }
}

/// Everything one authenticated bridge request commits to.
///
/// The previous preimage was `(bond, path, body_digest)`. Three consequences, all exploitable:
///
/// * **The QUERY was unsigned.** Identity on the read routes travels in the query
///   (`?provider_id=`, `?auditor_bond=`, `?provider_bond=`), so a signature said nothing about
///   who the request claimed to be — and those routes did not even call the authenticator, making
///   identity a self-declared string. Anyone could claim another provider's assignments, read its
///   audit prompts, or open its DA obligations.
/// * **The METHOD was unsigned**, so a signature for one verb on a path was valid for another.
/// * **Nothing expired or was nonced**, so any observed request was replayable forever.
#[derive(Clone, Copy, Debug)]
pub struct SignedRequest<'a> {
    pub network_id: u32,
    pub bond_outpoint: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    /// The request's query in canonical form ([`canonical_query`]), NOT the raw string: two
    /// orderings of the same parameters must not be two different requests to sign.
    pub canonical_query: &'a str,
    pub body_digest: &'a Hash64,
    /// Single-use, per bond. The bridge refuses a repeat within the acceptance window.
    pub nonce: &'a str,
    /// Unix ms after which this signature is refused, bounding both replay and clock skew.
    pub expires_at_unix_ms: i64,
}

/// The signing hash every authenticated bridge request commits to.
pub fn request_signing_hash(request: &SignedRequest<'_>) -> Hash64 {
    fn push_str(preimage: &mut Vec<u8>, value: &str) {
        preimage.extend_from_slice(&(value.len() as u64).to_le_bytes());
        preimage.extend_from_slice(value.as_bytes());
    }
    let mut preimage = Vec::with_capacity(128 + request.path.len() + request.canonical_query.len());
    preimage.extend_from_slice(&request.network_id.to_le_bytes());
    push_str(&mut preimage, request.bond_outpoint);
    push_str(&mut preimage, request.method);
    push_str(&mut preimage, request.path);
    push_str(&mut preimage, request.canonical_query);
    preimage.extend_from_slice(request.body_digest.as_byte_slice());
    push_str(&mut preimage, request.nonce);
    preimage.extend_from_slice(&request.expires_at_unix_ms.to_le_bytes());
    blake2b_512_keyed(BRIDGE_REQUEST_SIGNING_DOMAIN, &preimage)
}

/// `k=v` pairs sorted by `(key, value)` and rejoined with `&` — one canonical string per
/// parameter SET, so a client cannot reorder a signed query into a "different" request, and the
/// server cannot be fed a query whose meaning depends on parse order.
pub fn canonical_query(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(&str, &str)> =
        query.split('&').filter(|pair| !pair.is_empty()).map(|pair| pair.split_once('=').unwrap_or((pair, ""))).collect();
    pairs.sort_unstable();
    pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
}

pub fn body_digest(body: &[u8]) -> Hash64 {
    blake2b_512_keyed(b"misaka-palw-bridge-v1/request-body", body)
}

fn verify_mldsa87(public_key: &[u8], message: &Hash64, signature: &[u8], context: &[u8]) -> Result<(), String> {
    match kaspa_txscript::verify_mldsa87_with_context(public_key, message.as_byte_slice(), signature, context) {
        Ok(true) => Ok(()),
        Ok(false) => Err("ML-DSA-87 signature does not verify".into()),
        Err(e) => Err(format!("ML-DSA-87 verify: {e}")),
    }
}

/// Registration payload: prove the bond is yours, and delegate to a session key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderRegistrationV1 {
    pub bond_outpoint: String,
    pub owner_public_key_hex: String,
    /// Borsh-encoded `PalwProviderSessionAuthorizationV1`, signed by the OWNER key. The node's
    /// own delegation object — verified here with the node's own verifier.
    pub session_authorization_hex: String,
}

pub struct ProviderRegistry {
    providers: BTreeMap<String, RegisteredProvider>,
    network_id: u32,
}

impl ProviderRegistry {
    pub fn new(network_id: u32) -> Self {
        Self { providers: BTreeMap::new(), network_id }
    }

    pub fn get(&self, bond_outpoint: &str) -> Option<&RegisteredProvider> {
        self.providers.get(bond_outpoint)
    }

    pub fn all(&self) -> impl Iterator<Item = &RegisteredProvider> {
        self.providers.values()
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Insert a provider that has ALREADY been verified (journal replay path — the event was
    /// verified when it was first accepted, and the chain facts are re-checked on use).
    pub fn insert_verified(&mut self, provider: RegisteredProvider) {
        self.providers.insert(provider.bond_outpoint.clone(), provider);
    }

    /// Verify a registration against the chain and admit it.
    pub fn verify_registration(
        &self,
        registration: &ProviderRegistrationV1,
        chain: &dyn ChainFacts,
    ) -> Result<RegisteredProvider, String> {
        let owner_public_key = decode_hex(&registration.owner_public_key_hex)?;
        if owner_public_key.len() != MLDSA87_PK_LEN {
            return Err(format!("owner public key must be {MLDSA87_PK_LEN} bytes, got {}", owner_public_key.len()));
        }
        let bond = chain.bond_record(&registration.bond_outpoint)?;

        // (1) the presented key is the bond's key
        let credential = validator_id_from_pubkey(&owner_public_key);
        if hash64_hex(&credential) != bond.owner_pubkey_hash_hex {
            return Err("owner public key does not hash to the bond's owner_pubkey_hash".into());
        }
        // (2) the bond is ACTIVE — pending/unbonding/slashed providers do no work here
        if !bond.is_active() {
            return Err(format!("bond {} is {}, not active", registration.bond_outpoint, bond.effective_status));
        }
        // (2b) our reconstruction agrees with the node's own status derivation
        let record = bond.to_record(owner_public_key.clone())?;
        let beacon = chain.beacon()?;
        if !record_status_agrees(&record, &bond, beacon.observed_daa_score) {
            return Err("reconstructed bond record disagrees with the node's effective status".into());
        }
        if format_outpoint(&record.bond_outpoint) != registration.bond_outpoint {
            return Err("bond outpoint round-trip mismatch".into());
        }

        // (3) the session delegation — the node's object, the node's rules
        let auth_bytes = decode_hex(&registration.session_authorization_hex)?;
        let auth: PalwProviderSessionAuthorizationV1 =
            borsh::from_slice(&auth_bytes).map_err(|e| format!("session authorization decode: {e}"))?;
        if auth.network_id != self.network_id {
            return Err(format!("session authorization is for network {}, bridge is {}", auth.network_id, self.network_id));
        }
        if format_outpoint(&auth.provider_bond) != registration.bond_outpoint {
            return Err("session authorization names a different provider bond".into());
        }
        if auth.owner_public_key != owner_public_key {
            return Err("session authorization owner key differs from the presented owner key".into());
        }
        if auth.session_public_key.len() != MLDSA87_PK_LEN {
            return Err("session public key length".into());
        }
        if auth.authorization_nonce == Hash64::default() {
            return Err("session authorization nonce is zero".into());
        }
        if auth.valid_until_epoch < auth.valid_from_epoch {
            return Err("session authorization epoch range is inverted".into());
        }
        if !(auth.valid_from_epoch..=auth.valid_until_epoch).contains(&beacon.current_epoch) {
            return Err(format!(
                "session authorization covers epochs {}..={}, current epoch is {}",
                auth.valid_from_epoch, auth.valid_until_epoch, beacon.current_epoch
            ));
        }
        verify_mldsa87(
            &owner_public_key,
            &auth.signing_hash(),
            &auth.signature,
            kaspa_consensus_core::palw::da::PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT,
        )
        .map_err(|e| format!("session authorization signature: {e}"))?;

        Ok(RegisteredProvider {
            bond_outpoint: registration.bond_outpoint.clone(),
            owner_public_key_hex: registration.owner_public_key_hex.clone(),
            credential_hex: hash64_hex(&credential),
            session_public_key_hex: crate::match_key::bytes_hex(&auth.session_public_key),
            session_valid_from_epoch: auth.valid_from_epoch,
            session_valid_until_epoch: auth.valid_until_epoch,
            bond,
        })
    }

    /// Authenticate a request: the session key signed THIS body for THIS route, the session is
    /// unexpired, and the bond is still active on chain right now.
    pub fn authenticate(
        &self,
        request: &SignedRequest<'_>,
        signature_hex: &str,
        chain: &dyn ChainFacts,
        current_epoch: u64,
    ) -> Result<&RegisteredProvider, String> {
        let bond_outpoint = request.bond_outpoint;
        let provider =
            self.providers.get(bond_outpoint).ok_or_else(|| format!("provider {bond_outpoint} is not registered with this bridge"))?;
        if !provider.session_valid_at(current_epoch) {
            return Err(format!(
                "session key expired (valid {}..={}, now {current_epoch}) — re-register with a fresh authorization",
                provider.session_valid_from_epoch, provider.session_valid_until_epoch
            ));
        }
        let signature = decode_hex(signature_hex)?;
        if signature.len() != MLDSA87_SIG_LEN {
            return Err(format!("signature must be {MLDSA87_SIG_LEN} bytes, got {}", signature.len()));
        }
        let session_key = decode_hex(&provider.session_public_key_hex)?;
        let hash = request_signing_hash(request);
        verify_mldsa87(&session_key, &hash, &signature, BRIDGE_REQUEST_MLDSA87_CONTEXT)?;

        // Re-check the bond on chain: a provider slashed or unbonded since registration must
        // stop being able to submit or replicate.
        let live = chain.bond_record(bond_outpoint)?;
        if !live.is_active() {
            return Err(format!("bond {bond_outpoint} is now {}, not active", live.effective_status));
        }
        Ok(provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(mutate: impl FnOnce(&mut (String, String, String, String, Hash64, String, i64))) -> Hash64 {
        let mut parts = (
            "aa:0".to_string(),
            "GET".to_string(),
            "/palw/v1/jobs".to_string(),
            String::new(),
            body_digest(b"{\"a\":1}"),
            "nonce-1".to_string(),
            1_000i64,
        );
        mutate(&mut parts);
        request_signing_hash(&SignedRequest {
            network_id: 20,
            bond_outpoint: &parts.0,
            method: &parts.1,
            path: &parts.2,
            canonical_query: &parts.3,
            body_digest: &parts.4,
            nonce: &parts.5,
            expires_at_unix_ms: parts.6,
        })
    }

    /// BRIDGE-AUTH-01 — every component of a request is bound. The three that were NOT before
    /// (query, method, freshness) are the ones that made identity forgeable and requests
    /// replayable, so each gets its own assertion.
    #[test]
    fn signing_hash_binds_every_component_of_the_request() {
        let base = signed(|_| {});
        assert_eq!(base, signed(|_| {}), "deterministic");
        assert_ne!(base, signed(|p| p.0 = "bb:0".into()), "bond is bound");
        assert_ne!(base, signed(|p| p.1 = "POST".into()), "METHOD is bound");
        assert_ne!(base, signed(|p| p.2 = "/palw/v1/verdicts".into()), "path is bound");
        assert_ne!(base, signed(|p| p.3 = "provider_id=bb:0".into()), "QUERY is bound");
        assert_ne!(base, signed(|p| p.4 = body_digest(b"{\"a\":2}")), "body is bound");
        assert_ne!(base, signed(|p| p.5 = "nonce-2".into()), "NONCE is bound");
        assert_ne!(base, signed(|p| p.6 = 2_000), "EXPIRY is bound");
    }

    /// The query is signed in canonical form, so a client cannot reorder a signed query into a
    /// different request — nor can two orderings of one request need two signatures.
    #[test]
    fn query_canonicalization_is_order_independent() {
        assert_eq!(canonical_query("b=2&a=1"), "a=1&b=2");
        assert_eq!(canonical_query("a=1&b=2"), "a=1&b=2");
        assert_eq!(canonical_query(""), "");
        assert_eq!(canonical_query("flag"), "flag=");
        assert_eq!(
            signed(|p| p.3 = canonical_query("b=2&a=1")),
            signed(|p| p.3 = canonical_query("a=1&b=2")),
            "one signature covers the parameter SET, not one spelling of it"
        );
    }

    /// Length framing: adjacent fields must not be reinterpretable by moving the boundary.
    #[test]
    fn signing_hash_framing_is_unambiguous() {
        assert_ne!(
            signed(|p| {
                p.0 = "ab".into();
                p.2 = "cd".into();
            }),
            signed(|p| {
                p.0 = "a".into();
                p.2 = "bcd".into();
            })
        );
    }

    #[test]
    fn malformed_keys_and_signatures_are_refused() {
        let hash = signed(|p| p.2 = "/r".into());
        assert!(verify_mldsa87(&[0u8; 10], &hash, &[0u8; MLDSA87_SIG_LEN], BRIDGE_REQUEST_MLDSA87_CONTEXT).is_err());
        assert!(verify_mldsa87(&[0u8; MLDSA87_PK_LEN], &hash, &[0u8; 10], BRIDGE_REQUEST_MLDSA87_CONTEXT).is_err());
        // A well-formed but wrong key/sig pair verifies to false, not to an error.
        let err = verify_mldsa87(&[0u8; MLDSA87_PK_LEN], &hash, &[0u8; MLDSA87_SIG_LEN], BRIDGE_REQUEST_MLDSA87_CONTEXT).unwrap_err();
        assert!(err.contains("does not verify"), "{err}");
    }

    #[test]
    fn unregistered_provider_cannot_authenticate() {
        let registry = ProviderRegistry::new(1);
        let chain = crate::chain::PinnedChainFacts::from_parts(
            crate::chain::BeaconFacts {
                epoch: 1,
                seed_hex: "aa".repeat(64),
                anchor_hash_hex: "bb".repeat(64),
                anchor_daa_score: 100,
                observed_daa_score: 400,
                current_epoch: 4,
            },
            BTreeMap::new(),
        );
        let request = SignedRequest {
            network_id: 20,
            bond_outpoint: "nope:0",
            method: "POST",
            path: "/palw/v1/jobs",
            canonical_query: "",
            body_digest: &body_digest(b"{}"),
            nonce: "nonce-1",
            expires_at_unix_ms: 1_000,
        };
        let err = registry.authenticate(&request, &"00".repeat(MLDSA87_SIG_LEN), &chain, 4).unwrap_err();
        assert!(err.contains("not registered"), "{err}");
    }
}
