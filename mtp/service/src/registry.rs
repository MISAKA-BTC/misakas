//! Registration + attribution (ADR-0038 D4: I-MTP-1, I-MTP-4, I-MTP-11).
//!
//! Three trust-critical service-layer controls live here, all pure/deterministic
//! so they are unit-tested:
//!
//! * **[`NonceStore`] (I-MTP-4)** — server-issued 32-byte challenge nonces, bound
//!   to a `(github, address)` pair, 7-day TTL, single-use, deleted on success
//!   or expiry. The challenge bytes are the canonical Appendix-B registration
//!   message; the core [`misaka_mtp::verify_registration`] checks the ML-DSA-87
//!   signature over exactly those bytes.
//! * **[`claim_token`] (I-MTP-11)** — a short deterministic token derived from the
//!   registration record. The participant configures their node to advertise
//!   `mtp:<token>` in its P2P user-agent comment; ingestion extracts it from the
//!   crawler-observed user-agent ([`extract_claim_token`]) and attributes uptime to
//!   the registration it resolves to (possession-of-config binding).
//! * **[`Attributor`] (I-MTP-1 / G1)** — the single attribution authority. Every
//!   scoreable fact must resolve, through a registration, to the one canonical
//!   ledger id `gh:<handle>`; unresolvable facts are **dropped, not bucketed**
//!   (fail-closed). This is what closes identity-namespace splitting: one human's
//!   GitHub handle, address, and node token all collapse to a single ledger id, so
//!   they can no longer defeat `d_n` or the 5 % settlement cap by fanning out.

use kaspa_addresses::Prefix;
use kaspa_hashes::blake2b_512_keyed;
use misaka_mtp::{LedgerAttribution, Registration, RegistrationError, verify_registration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::MTP_CLAIM_TOKEN_CONTEXT;

/// Nonce time-to-live: 7 days (I-MTP-4).
///
/// The invitation travels through an asynchronous GitHub issue/PR round-trip
/// (operator issues → participant signs offline → participant posts the result →
/// operator ingests), so the TTL must cover days, not minutes — the original
/// 15-minute TTL made every real-world registration expire before ingestion.
/// Replay is not loosened by the longer window: the nonce is single-use, bound to
/// its `(github, address)` pair, and useless without the key that derives the
/// invited address (the signature binds pubkey ↔ address ↔ challenge).
pub const NONCE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
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
    /// Which identity this human's points accrue to — their signed choice. `#[serde(default)]`
    /// means every record written before the choice existed reads back as `Github`, so replaying
    /// `registrations.jsonl` reproduces exactly the ledger ids it produced before.
    #[serde(default)]
    pub attribution: LedgerAttribution,
}

impl RegistrationRecord {
    /// The single canonical ledger id every fact for this human resolves to.
    ///
    /// Still ONE id per registration — the choice picks its spelling, it does not create a second
    /// bucket: address, claim-token and handle lookups all resolve to this same string, so a
    /// participant cannot be paid twice by arriving through two different facts.
    pub fn ledger_id(&self) -> String {
        match self.attribution {
            LedgerAttribution::Github => format!("gh:{}", self.github),
            LedgerAttribution::Address => format!("addr:{}", self.address),
        }
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

/// Extract the I-MTP-11 claim-token from a crawler-observed P2P user-agent.
///
/// The participant advertises it via `--uacomment=mtp:<token>`, which the node
/// renders inside its user-agent as `.../name:version(mtp:<token>; other)/`. This
/// scanner accepts the token anywhere in the string, requires exactly
/// [`CLAIM_TOKEN_BYTES`]·2 hex chars (a truncated user-agent yields no token, not
/// a wrong one — fail-closed), and lowercases it to match [`claim_token`] output.
/// A longer hex run is not a token (no prefix-taking); scanning continues, so one
/// malformed comment cannot mask a well-formed one later in the string.
pub fn extract_claim_token(user_agent: &str) -> Option<ClaimToken> {
    let mut rest = user_agent;
    while let Some(pos) = rest.find("mtp:") {
        let after = &rest[pos + 4..];
        let end = after.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(after.len());
        if end == CLAIM_TOKEN_BYTES * 2 {
            return Some(after[..end].to_ascii_lowercase());
        }
        rest = &after[end..];
    }
    None
}

// The registration challenge moved to `misaka_mtp::registry` so the signer (a participant's CLI)
// and the verifier (this crate) share one definition instead of two that can drift. Re-exported so
// existing callers keep working.
pub use misaka_mtp::registry::{registration_challenge, registration_challenge_for};

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
        attribution: LedgerAttribution,
    ) -> Result<RegistrationRecord, AttributionError> {
        if self.by_github.contains_key(github) {
            return Err(AttributionError::DuplicateGithub);
        }
        if self.by_address.contains_key(address) {
            return Err(AttributionError::DuplicateAddress);
        }
        let issued_at = nonces.consume(github, address, nonce_hex, now_ms)?;
        // The challenge carries the attribution choice, so a signature produced for one choice
        // cannot be admitted under the other — the operator ingests this file but cannot edit
        // where the points land.
        let challenge = registration_challenge_for(network, github, address, nonce_hex, issued_at, attribution);
        let Registration { .. } = verify_registration(github, address, pubkey, &challenge, signature, prefix)?;
        let record = RegistrationRecord {
            github: github.to_string(),
            address: address.to_string(),
            pubkey: pubkey.to_vec(),
            claim_token: claim_token(github, address),
            registered_at_ms: now_ms,
            attribution,
        };
        self.index(&record);
        self.records.push(record.clone());
        Ok(record)
    }

    /// Resolve an on-chain address to its canonical ledger id (chain/campaign facts).
    ///
    /// **2026-08-02: registration-free.** The id is derived from the address itself
    /// ([`misaka_mtp::registry::ledger_id_for_address`]); a registration is no longer consulted and
    /// no longer required. Using a testnet address IS the enrolment. The registration index is kept
    /// only for the pre-change ledgers and for the operator dashboard.
    ///
    /// Returns an owned String rather than a borrow because the id is now computed, not stored.
    pub fn resolve_address_id(&self, address: &str) -> Option<String> {
        misaka_mtp::registry::ledger_id_for_address(address)
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
        // Both spellings are canonical ids, but each is only live for a registration that CHOSE it:
        // `gh:alice` is not a valid id for a human who registered under `addr:…`, or the fail-closed
        // membership test would admit a fact bucketed under an id nothing ever pays out to.
        // 2026-08-02: an `addr:` id is live iff it is a well-formed address. That is the whole
        // point of the change — the fail-closed membership test used to drop the facts of anyone
        // who earned on-chain without registering, which is a way to lose points, not a way to
        // prevent fraud.
        if let Some(address) = ledger_id.strip_prefix("addr:") {
            return misaka_mtp::registry::ledger_id_for_address(address).as_deref() == Some(ledger_id);
        }
        // `gh:` ids remain resolvable ONLY for registrations made before the change, so historical
        // ledgers stay verifiable. Nothing new is issued under this prefix.
        if let Some(handle) = ledger_id.strip_prefix("gh:") {
            return self.by_github.get(handle).is_some_and(|id| id == ledger_id);
        }
        false
    }

    /// All registrations (for persistence / the operator dashboard).
    pub fn records(&self) -> &[RegistrationRecord] {
        &self.records
    }
}

/// The C5 attribution seam: a provider's bond-owner address resolves to the same canonical ledger
/// id every other fact for that human resolves to.
///
/// This is the last link of the ADR-0040 §16″ chain — worker credential → provider bond → bond
/// owner address → **registered MTP id** — and it is deliberately the SAME index the crawler and
/// campaign facts use, so one human cannot appear as two participants by earning through a
/// different category.
impl misaka_mtp_collectors::OwnerResolver for Attributor {
    fn ledger_id_for_address(&self, address: &str) -> Option<String> {
        self.resolve_address_id(address)
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
    fn claim_token_extraction_from_user_agents() {
        let tok = claim_token("alice", "misakatest:aaa");
        // The shapes a real node produces: sole comment, alongside other comments, either order.
        let ua = format!("/kaspad:1.0.1/kaspad:1.0.1(mtp:{tok})/");
        assert_eq!(extract_claim_token(&ua).as_deref(), Some(tok.as_str()));
        let ua = format!("/kaspad:1.0.1/kaspad:1.0.1(oracle-arm; mtp:{tok}; jp)/");
        assert_eq!(extract_claim_token(&ua).as_deref(), Some(tok.as_str()));
        // Uppercase paste still resolves (normalized to the claim_token alphabet).
        let ua = format!("/kaspad:1.0.1(mtp:{})/", tok.to_ascii_uppercase());
        assert_eq!(extract_claim_token(&ua).as_deref(), Some(tok.as_str()));
        // A malformed candidate earlier in the string cannot mask a valid one later.
        let ua = format!("/kaspad:1.0.1(mtp:deadbeef; mtp:{tok})/");
        assert_eq!(extract_claim_token(&ua).as_deref(), Some(tok.as_str()));
        // Fail-closed: absent, truncated (user-agent length cap), or over-long runs are no token.
        assert_eq!(extract_claim_token("/kaspad:1.0.1/"), None);
        assert_eq!(extract_claim_token(&format!("/kaspad:1.0.1(mtp:{})", &tok[..10])), None);
        assert_eq!(extract_claim_token(&format!("/kaspad:1.0.1(mtp:{tok}0)/")), None);
        assert_eq!(extract_claim_token("mtp:"), None);
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

        let rec = attr
            .register(&mut ns, "testnet-10", "alice", &addr, &pk, &nonce_hex, &sig, 1000, Prefix::Testnet, LedgerAttribution::Github)
            .unwrap();
        assert_eq!(rec.ledger_id(), "gh:alice");
        // The token and handle namespaces still resolve to the registration's canonical id.
        assert_eq!(attr.resolve_token(&rec.claim_token), Some("gh:alice"));
        assert_eq!(attr.resolve_github("alice"), Some("gh:alice"));
        // 2026-08-02: the ADDRESS namespace no longer consults the registration — it credits the
        // address itself, so a `gh:`-attributed registration no longer renames on-chain earnings.
        assert_eq!(attr.resolve_address_id(&addr).as_deref(), Some(format!("addr:{addr}").as_str()));
        // A token that was never issued still resolves to nothing (that namespace is unchanged).
        assert_eq!(attr.resolve_token("deadbeef"), None);
    }

    /// The participant CHOOSES where their points accrue, and that choice is signed: a request
    /// signed for `Address` must not be admissible as a `Github` registration, or an operator could
    /// silently redirect someone's rewards by editing one JSON field in transit.
    #[test]
    fn attribution_choice_is_signed_and_selects_the_ledger_id() {
        let (key, pk, addr) = key_and_addr(0x53);
        let mut ns = NonceStore::new();
        let mut attr = Attributor::new();
        let nonce = [0x77u8; 32];
        let nonce_hex = faster_hex::hex_string(&nonce);
        ns.issue("testnet-21", "dora", &addr, nonce, 1000);
        // The participant signs the v2 (address-attribution) challenge.
        let challenge = registration_challenge_for("testnet-21", "dora", &addr, &nonce_hex, 1000, LedgerAttribution::Address);
        let sig = key.sign_with_context(&challenge, MTP_REGISTER_CONTEXT);

        // Admitting it as a `Github` registration must fail — different bytes, so the signature
        // cannot verify. The nonce is consumed by the attempt (fail-closed against replay), so the
        // honest admission below re-issues it, exactly as a participant would have to.
        let err = attr
            .register(&mut ns, "testnet-21", "dora", &addr, &pk, &nonce_hex, &sig, 1000, Prefix::Testnet, LedgerAttribution::Github)
            .unwrap_err();
        assert!(matches!(err, AttributionError::Registration(_)), "tampered attribution must not verify, got {err:?}");

        ns.issue("testnet-21", "dora", &addr, nonce, 1000);
        let rec = attr
            .register(&mut ns, "testnet-21", "dora", &addr, &pk, &nonce_hex, &sig, 1000, Prefix::Testnet, LedgerAttribution::Address)
            .unwrap();
        assert_eq!(rec.ledger_id(), format!("addr:{addr}"));
        // 2026-08-02: address resolution no longer consults the registration at all, so it agrees
        // with an address-attributed record by construction rather than by lookup.
        assert_eq!(attr.resolve_address_id(&addr).as_deref(), Some(rec.ledger_id().as_str()));
        assert_eq!(attr.resolve_token(&rec.claim_token), Some(rec.ledger_id().as_str()));
        assert_eq!(attr.resolve_github("dora"), Some(rec.ledger_id().as_str()));
        assert!(attr.is_registered_id(&rec.ledger_id()));
        // A `gh:` id this human declined is still not live for them — the legacy prefix resolves
        // only for registrations that actually chose it.
        assert!(!attr.is_registered_id("gh:dora"));
    }

    /// A record written before the choice existed reads back as `Github` — replaying an older
    /// `registrations.jsonl` must reproduce exactly the ledger ids it produced before.
    #[test]
    fn stored_records_without_attribution_stay_github() {
        let legacy = r#"{"github":"eve","address":"misakatest:eve","pubkey":[],"claim_token":"t","registered_at_ms":7}"#;
        let rec: RegistrationRecord = serde_json::from_str(legacy).expect("legacy record parses");
        assert_eq!(rec.attribution, LedgerAttribution::Github);
        assert_eq!(rec.ledger_id(), "gh:eve");
    }

    /// 2026-08-02 — C5 attribution is registration-free: the provider's bond-owner ADDRESS is the
    /// ledger id. An operator who earns on-chain without ever touching the registration service is
    /// credited, which is the whole point of the change; the old behaviour dropped their facts.
    #[test]
    fn c5_owner_resolution_is_registration_free() {
        use misaka_mtp_collectors::OwnerResolver;

        let (key, pk, addr) = key_and_addr(0x43);
        let mut ns = NonceStore::new();
        let mut attr = Attributor::new();
        let nonce = [0x33u8; 32];
        let nonce_hex = faster_hex::hex_string(&nonce);
        let ch = ns.issue("testnet-20", "alice", &addr, nonce, 1000);
        let sig = key.sign_with_context(&ch, MTP_REGISTER_CONTEXT);
        attr.register(&mut ns, "testnet-20", "alice", &addr, &pk, &nonce_hex, &sig, 1000, Prefix::Testnet, LedgerAttribution::Github)
            .unwrap();

        // Even with a `gh:`-attributed registration on file, the C5 seam credits the ADDRESS: the
        // chain knows who did the work, and that is now the only thing consulted.
        assert_eq!(attr.ledger_id_for_address(&addr).as_deref(), Some(format!("addr:{addr}").as_str()));
        assert_eq!(attr.ledger_id_for_address(&addr), attr.resolve_address_id(&addr));
        // A provider who never registered is now CREDITED rather than dropped — the regression this
        // change exists to fix.
        let stranger = "misakatest:qstranger";
        assert_eq!(attr.ledger_id_for_address(stranger).as_deref(), Some("addr:misakatest:qstranger"));
        assert!(attr.is_registered_id("addr:misakatest:qstranger"), "a used address is live without registering");
        // Malformed input is still refused, so a bad fact cannot invent an id.
        assert_eq!(attr.ledger_id_for_address("not-an-address"), None);
        assert_eq!(attr.ledger_id_for_address("bitcoin:qabc"), None);
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
        attr.register(&mut ns, "testnet-10", "bob", &addr, &pk, &nonce_hex, &sig, 1, Prefix::Testnet, LedgerAttribution::Github)
            .unwrap();

        // same handle again → DuplicateGithub (one human, one ledger id).
        ns.issue("testnet-10", "bob", &addr, nonce, 2);
        let err = attr
            .register(&mut ns, "testnet-10", "bob", &addr, &pk, &nonce_hex, &sig, 2, Prefix::Testnet, LedgerAttribution::Github)
            .unwrap_err();
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
        let err = attr
            .register(&mut ns, "testnet-10", "carol", &addr, &pk, &nonce_hex, &sig, 1, Prefix::Testnet, LedgerAttribution::Github)
            .unwrap_err();
        assert_eq!(err, AttributionError::Registration(RegistrationError::BadSignature));
    }
}
