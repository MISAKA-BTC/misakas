//! Registration + attribution (ADR-0038 D4: I-MTP-1, I-MTP-4, I-MTP-11).
//!
//! Three trust-critical service-layer controls live here, all pure/deterministic
//! so they are unit-tested:
//!
//! * **[`NonceStore`] (I-MTP-4)** — server-issued 32-byte challenge nonces, bound
//!   to a `(github, address)` pair, 15-minute TTL, single-use, deleted on success
//!   or expiry. The challenge bytes are the canonical Appendix-B registration
//!   message; the core [`misaka_mtp::verify_registration`] checks the ML-DSA-87
//!   signature over exactly those bytes.
//! * **[`claim_token`] (I-MTP-11)** — a short deterministic token derived from the
//!   registration record. The participant configures their node to advertise
//!   `mtp:<token>` in its P2P user-agent comment; the crawler attributes uptime to
//!   a registration only when it observes the token (possession-of-config binding).
//! * **[`Attributor`] (I-MTP-1 / G1)** — the single attribution authority. Every
//!   scoreable fact must resolve, through a registration, to the one canonical
//!   ledger id `gh:<handle>`; unresolvable facts are **dropped, not bucketed**
//!   (fail-closed). This is what closes identity-namespace splitting: one human's
//!   GitHub handle, address, and node token all collapse to a single ledger id, so
//!   they can no longer defeat `d_n` or the 5 % settlement cap by fanning out.

use kaspa_addresses::Prefix;
use kaspa_hashes::blake2b_512_keyed;
use misaka_mtp::{Registration, RegistrationError, verify_registration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::MTP_CLAIM_TOKEN_CONTEXT;

/// Nonce time-to-live: 15 minutes (I-MTP-4).
pub const NONCE_TTL_MS: u64 = 15 * 60 * 1000;
/// Claim-token length in bytes (24 hex chars) — short enough for a user-agent
/// comment, wide enough that a collision across registrations is negligible.
pub const CLAIM_TOKEN_BYTES: usize = 12;

/// A verified registration plus its service-layer bindings (D4). The canonical
/// ledger id is `gh:<github>`; the claim-token binds owned nodes (I-MTP-11).
/// Persisted as JSONL so the service can rebuild its [`Attributor`] on restart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationRecord {
    pub github: String,
    pub address: String,
    pub pubkey: Vec<u8>,
    pub claim_token: String,
    pub registered_at_ms: u64,
}

impl RegistrationRecord {
    /// The single canonical ledger id every fact for this human resolves to.
    pub fn ledger_id(&self) -> String {
        format!("gh:{}", self.github)
    }
}

/// A [`ClaimToken`] is just a hex string; the newtype documents intent at call sites.
pub type ClaimToken = String;

/// Derive the deterministic per-registration claim-token (I-MTP-11) from the
/// stable identity fields. Anyone can recompute it, but only someone who can edit
/// the node's config can make the node advertise it — which is exactly the
/// possession direction we want (you cannot claim a node you do not operate).
pub fn claim_token(github: &str, address: &str) -> ClaimToken {
    let mut preimage = Vec::new();
    preimage.extend_from_slice(github.as_bytes());
    preimage.push(0x00);
    preimage.extend_from_slice(address.as_bytes());
    let h = blake2b_512_keyed(MTP_CLAIM_TOKEN_CONTEXT, &preimage);
    faster_hex::hex_string(&h.as_bytes()[..CLAIM_TOKEN_BYTES])
}

// The registration challenge moved to `misaka_mtp::registry` so the signer (a participant's CLI)
// and the verifier (this crate) share one definition instead of two that can drift. Re-exported so
// existing callers keep working.
pub use misaka_mtp::registry::registration_challenge;

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum NonceError {
    #[error("no such nonce was issued (or it was already used)")]
    Unknown,
    #[error("nonce was issued for a different (github, address) pair")]
    PairMismatch,
    #[error("nonce expired (issued more than the TTL ago)")]
    Expired,
}

struct NonceEntry {
    github: String,
    address: String,
    issued_at_ms: u64,
}

/// Server-issued, pair-bound, single-use, TTL-limited challenge nonces (I-MTP-4).
#[derive(Default)]
pub struct NonceStore {
    entries: HashMap<String, NonceEntry>,
    ttl_ms: u64,
}

impl NonceStore {
    pub fn new() -> Self {
        Self { entries: HashMap::new(), ttl_ms: NONCE_TTL_MS }
    }

    /// A store with a custom TTL (tests).
    pub fn with_ttl(ttl_ms: u64) -> Self {
        Self { entries: HashMap::new(), ttl_ms }
    }

    /// Register a freshly generated 32-byte nonce for `(github, address)` and
    /// return the challenge bytes the participant must sign. The caller supplies
    /// the random bytes (the RNG lives at the service edge) and the clock.
    pub fn issue(&mut self, network: &str, github: &str, address: &str, nonce: [u8; 32], now_ms: u64) -> Vec<u8> {
        let nonce_hex = faster_hex::hex_string(&nonce);
        let challenge = registration_challenge(network, github, address, &nonce_hex, now_ms);
        self.entries.insert(nonce_hex, NonceEntry { github: github.to_string(), address: address.to_string(), issued_at_ms: now_ms });
        challenge
    }

    /// Consume a nonce: it must exist, match the `(github, address)` it was issued
    /// for, and be within TTL. On any outcome the nonce is **removed** (single-use:
    /// a replay after success finds `Unknown`). Returns the `issued_at_ms` so the
    /// caller can recompute the exact challenge for signature verification.
    pub fn consume(&mut self, github: &str, address: &str, nonce_hex: &str, now_ms: u64) -> Result<u64, NonceError> {
        let Some(entry) = self.entries.remove(nonce_hex) else {
            return Err(NonceError::Unknown);
        };
        if entry.github != github || entry.address != address {
            return Err(NonceError::PairMismatch);
        }
        if now_ms.saturating_sub(entry.issued_at_ms) > self.ttl_ms {
            return Err(NonceError::Expired);
        }
        Ok(entry.issued_at_ms)
    }

    /// Drop every expired nonce (call periodically; consume also self-cleans).
    pub fn gc(&mut self, now_ms: u64) {
        let ttl = self.ttl_ms;
        self.entries.retain(|_, e| now_ms.saturating_sub(e.issued_at_ms) <= ttl);
    }

    /// Number of live (issued, unconsumed) nonces.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum AttributionError {
    #[error("nonce error: {0}")]
    Nonce(#[from] NonceError),
    #[error("registration signature/binding error: {0}")]
    Registration(#[from] RegistrationError),
    #[error("github handle already registered")]
    DuplicateGithub,
    #[error("address already registered")]
    DuplicateAddress,
}

/// The single attribution authority (I-MTP-1 / G1). Holds the registrations and
/// resolves any fact author-key — an on-chain address, a node claim-token, or a
/// GitHub handle — to the one canonical ledger id, or `None` (drop, fail-closed).
#[derive(Default)]
pub struct Attributor {
    records: Vec<RegistrationRecord>,
    by_address: HashMap<String, String>, // address   → gh:<handle>
    by_token: HashMap<String, String>,   // claim_tok → gh:<handle>
    by_github: HashMap<String, String>,  // handle    → gh:<handle>
}

impl Attributor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build an attributor from existing registration records (e.g. loaded from
    /// disk). Later duplicates on the same key are ignored (first-registered wins).
    pub fn from_records(records: Vec<RegistrationRecord>) -> Self {
        let mut a = Self::new();
        for r in records {
            a.index(&r);
            a.records.push(r);
        }
        a
    }

    fn index(&mut self, r: &RegistrationRecord) {
        let id = r.ledger_id();
        self.by_address.entry(r.address.clone()).or_insert_with(|| id.clone());
        self.by_token.entry(r.claim_token.clone()).or_insert_with(|| id.clone());
        self.by_github.entry(r.github.clone()).or_insert(id);
    }

    /// Verify a registration end-to-end and add it: consume the nonce (I-MTP-4),
    /// recompute the exact challenge, check the ML-DSA-87 binding + signature via
    /// the core, mint the claim-token (I-MTP-11), and index it (I-MTP-1). Returns
    /// the new record. Rejects duplicate github/address so one human's ledger id
    /// stays single.
    #[allow(clippy::too_many_arguments)]
    pub fn register(
        &mut self,
        nonces: &mut NonceStore,
        network: &str,
        github: &str,
        address: &str,
        pubkey: &[u8],
        nonce_hex: &str,
        signature: &[u8],
        now_ms: u64,
        prefix: Prefix,
    ) -> Result<RegistrationRecord, AttributionError> {
        if self.by_github.contains_key(github) {
            return Err(AttributionError::DuplicateGithub);
        }
        if self.by_address.contains_key(address) {
            return Err(AttributionError::DuplicateAddress);
        }
        let issued_at = nonces.consume(github, address, nonce_hex, now_ms)?;
        let challenge = registration_challenge(network, github, address, nonce_hex, issued_at);
        let Registration { .. } = verify_registration(github, address, pubkey, &challenge, signature, prefix)?;
        let record = RegistrationRecord {
            github: github.to_string(),
            address: address.to_string(),
            pubkey: pubkey.to_vec(),
            claim_token: claim_token(github, address),
            registered_at_ms: now_ms,
        };
        self.index(&record);
        self.records.push(record.clone());
        Ok(record)
    }

    /// Resolve an on-chain address to its canonical ledger id (chain/campaign facts).
    pub fn resolve_address(&self, address: &str) -> Option<&str> {
        self.by_address.get(address).map(String::as_str)
    }

    /// Resolve a node claim-token to its canonical ledger id (crawler facts, I-MTP-11).
    pub fn resolve_token(&self, token: &str) -> Option<&str> {
        self.by_token.get(token).map(String::as_str)
    }

    /// Resolve a GitHub handle to its canonical ledger id (bug/verify facts).
    pub fn resolve_github(&self, handle: &str) -> Option<&str> {
        self.by_github.get(handle).map(String::as_str)
    }

    /// Whether `ledger_id` is a currently-registered canonical id (`gh:<handle>`
    /// for a known handle). This is the fail-closed membership test the epoch
    /// builder applies to every fact (I-MTP-1): a fact carrying any id that does
    /// not resolve to a live registration is dropped, never scored.
    pub fn is_registered_id(&self, ledger_id: &str) -> bool {
        ledger_id.strip_prefix("gh:").map(|h| self.by_github.contains_key(h)).unwrap_or(false)
    }

    /// All registrations (for persistence / the operator dashboard).
    pub fn records(&self) -> &[RegistrationRecord] {
        &self.records
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_addresses::{Address, Version};
    use kaspa_hashes::blake2b_512_address_payload;
    use kaspa_pq_validator_core::ValidatorKey;
    use misaka_mtp::MTP_REGISTER_CONTEXT;

    fn key_and_addr(seed: u8) -> (ValidatorKey, Vec<u8>, String) {
        let key = ValidatorKey::from_seed([seed; 32]);
        let pk = key.public_key().to_vec();
        let payload = blake2b_512_address_payload(&pk);
        let addr = Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &payload.as_bytes());
        (key, pk, addr.to_string())
    }

    #[test]
    fn claim_token_is_deterministic_and_pair_specific() {
        let t1 = claim_token("alice", "misakatest:aaa");
        assert_eq!(t1, claim_token("alice", "misakatest:aaa"));
        assert_ne!(t1, claim_token("alice", "misakatest:bbb"));
        assert_ne!(t1, claim_token("bob", "misakatest:aaa"));
        assert_eq!(t1.len(), CLAIM_TOKEN_BYTES * 2);
    }

    #[test]
    fn nonce_is_single_use_pair_bound_and_expiring() {
        let mut ns = NonceStore::with_ttl(1000);
        ns.issue("testnet-10", "alice", "addrA", [7; 32], 100);
        let nonce_hex = faster_hex::hex_string(&[7u8; 32]);
        // wrong pair rejected (and the nonce is now consumed — reissue needed).
        assert_eq!(ns.consume("mallory", "addrA", &nonce_hex, 200), Err(NonceError::PairMismatch));
        // re-issue; correct pair within TTL succeeds once.
        ns.issue("testnet-10", "alice", "addrA", [7; 32], 100);
        assert_eq!(ns.consume("alice", "addrA", &nonce_hex, 500), Ok(100));
        // replay after success → Unknown (single-use).
        assert_eq!(ns.consume("alice", "addrA", &nonce_hex, 500), Err(NonceError::Unknown));
        // expired.
        ns.issue("testnet-10", "alice", "addrA", [7; 32], 100);
        assert_eq!(ns.consume("alice", "addrA", &nonce_hex, 100 + 1001), Err(NonceError::Expired));
    }

    #[test]
    fn full_registration_binds_and_indexes() {
        let (key, pk, addr) = key_and_addr(0x41);
        let mut ns = NonceStore::new();
        let mut attr = Attributor::new();
        let nonce = [0x11u8; 32];
        let nonce_hex = faster_hex::hex_string(&nonce);
        let challenge = ns.issue("testnet-10", "alice", &addr, nonce, 1000);
        let sig = key.sign_with_context(&challenge, MTP_REGISTER_CONTEXT);

        let rec = attr.register(&mut ns, "testnet-10", "alice", &addr, &pk, &nonce_hex, &sig, 1000, Prefix::Testnet).unwrap();
        assert_eq!(rec.ledger_id(), "gh:alice");
        // every namespace resolves to the ONE canonical id (G1/I-MTP-1).
        assert_eq!(attr.resolve_address(&addr), Some("gh:alice"));
        assert_eq!(attr.resolve_token(&rec.claim_token), Some("gh:alice"));
        assert_eq!(attr.resolve_github("alice"), Some("gh:alice"));
        // an unregistered key resolves to nothing → fact would be dropped.
        assert_eq!(attr.resolve_address("misakatest:stranger"), None);
        assert_eq!(attr.resolve_token("deadbeef"), None);
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let (key, pk, addr) = key_and_addr(0x42);
        let mut ns = NonceStore::new();
        let mut attr = Attributor::new();
        let nonce = [0x22u8; 32];
        let nonce_hex = faster_hex::hex_string(&nonce);
        let ch = ns.issue("testnet-10", "bob", &addr, nonce, 1);
        let sig = key.sign_with_context(&ch, MTP_REGISTER_CONTEXT);
        attr.register(&mut ns, "testnet-10", "bob", &addr, &pk, &nonce_hex, &sig, 1, Prefix::Testnet).unwrap();

        // same handle again → DuplicateGithub (one human, one ledger id).
        ns.issue("testnet-10", "bob", &addr, nonce, 2);
        let err = attr.register(&mut ns, "testnet-10", "bob", &addr, &pk, &nonce_hex, &sig, 2, Prefix::Testnet).unwrap_err();
        assert_eq!(err, AttributionError::DuplicateGithub);
    }

    #[test]
    fn registration_under_wrong_context_is_rejected() {
        use misaka_mtp::MTP_CLAIM_CONTEXT;
        let (key, pk, addr) = key_and_addr(0x43);
        let mut ns = NonceStore::new();
        let mut attr = Attributor::new();
        let nonce = [0x33u8; 32];
        let nonce_hex = faster_hex::hex_string(&nonce);
        let ch = ns.issue("testnet-10", "carol", &addr, nonce, 1);
        // sign under the CLAIM context, not REGISTER → must fail the binding check.
        let sig = key.sign_with_context(&ch, MTP_CLAIM_CONTEXT);
        let err = attr.register(&mut ns, "testnet-10", "carol", &addr, &pk, &nonce_hex, &sig, 1, Prefix::Testnet).unwrap_err();
        assert_eq!(err, AttributionError::Registration(RegistrationError::BadSignature));
    }
}
