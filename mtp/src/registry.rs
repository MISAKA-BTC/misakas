//! Registration + claim (ADR-0027 §5, §6.4, §11-B). A participant binds a GitHub
//! handle to a `misakatest:` ML-DSA-87 key by signing a server-issued challenge;
//! at TGE the same key signs the mainnet receiving address (PQ-key continuity).

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_hashes::{Hash64, blake2b_512_address_payload};
use kaspa_txscript::verify_mldsa87_with_context;

use crate::{MTP_CLAIM_CONTEXT, MTP_REGISTER_CONTEXT};

/// ML-DSA-87 verification-key length.
pub const MLDSA87_PK_LEN: usize = 2592;
/// ML-DSA-87 signature length.
pub const MLDSA87_SIG_LEN: usize = 4627;

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum RegistrationError {
    #[error("address does not parse")]
    BadAddress,
    #[error("address prefix is not the expected testnet prefix")]
    WrongPrefix,
    #[error("address is not a v2 ML-DSA-87 P2PKH (64-byte payload)")]
    WrongVersion,
    #[error("pubkey is not {MLDSA87_PK_LEN} bytes")]
    BadPubkeyLen,
    #[error("pubkey does not hash to the address payload")]
    KeyAddressMismatch,
    #[error("signature does not verify over the challenge")]
    BadSignature,
}

/// A verified registration binding a GitHub handle to an ML-DSA-87 identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    pub github: String,
    pub address: String,
    pub pubkey: Vec<u8>,
}

/// Which identity a participant's points accrue to — THEIR choice, not the operator's.
///
/// Both spellings name the same human (one registration binds one GitHub handle to one address),
/// so this changes only the ledger id the epoch builder buckets facts under, and therefore the name
/// a payout is published against. `Github` keeps points portable across the addresses that human may
/// rotate through; `Address` keeps them attached to the on-chain identity and away from a platform
/// account, which is the right answer for anyone who would rather not tie rewards to GitHub.
///
/// The choice is BOUND INTO THE SIGNED CHALLENGE (see [`registration_challenge_for`]) precisely
/// because it decides where value lands: the operator ingests a file the participant signed, and
/// must not be able to edit that field in flight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedgerAttribution {
    /// Points accrue to `gh:<handle>` — the historical (and still default) behaviour.
    #[default]
    Github,
    /// Points accrue to `addr:<misakatest:…>`.
    Address,
}

impl LedgerAttribution {
    /// The wire/JSON spelling accepted from a participant's request and the CLI flag.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Address => "address",
        }
    }

    /// Parse the participant-facing spelling. Absent ⇒ `Github`, so every pre-existing request,
    /// stored record and in-flight invitation keeps its current meaning.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "github" => Some(Self::Github),
            "address" => Some(Self::Address),
            _ => None,
        }
    }
}

/// Bind `pubkey` to `address_str`: the address must be a testnet v2 ML-DSA-87
/// P2PKH whose 64-byte payload equals `blake2b_512_address_payload(pubkey)`.
/// (The address payload is a HASH, not the pubkey — the full 2592-byte pubkey
/// must be transmitted separately, §4 note.)
fn bind_key_to_address(address_str: &str, pubkey: &[u8], expected_prefix: Prefix) -> Result<(), RegistrationError> {
    let addr = Address::try_from(address_str).map_err(|_| RegistrationError::BadAddress)?;
    if addr.prefix != expected_prefix {
        return Err(RegistrationError::WrongPrefix);
    }
    if addr.version != Version::PubKeyHashMlDsa87 {
        return Err(RegistrationError::WrongVersion);
    }
    if pubkey.len() != MLDSA87_PK_LEN {
        return Err(RegistrationError::BadPubkeyLen);
    }
    let addr_payload: [u8; 64] = (&addr.payload[..]).try_into().map_err(|_| RegistrationError::WrongVersion)?;
    if blake2b_512_address_payload(pubkey) != Hash64::from_bytes(addr_payload) {
        return Err(RegistrationError::KeyAddressMismatch);
    }
    Ok(())
}

/// The canonical Appendix-B registration challenge — the exact bytes a participant's key signs.
///
/// Deterministic in every field, so the issuing side and the verifying side recompute the same
/// message; changing network/github/address/nonce/issued_at flips the signature.
///
/// It lives HERE, beside [`verify_registration`], rather than in the service crate: the signer and
/// the verifier are now different programs (a participant's CLI, the operator's ingest), and a
/// challenge builder able to drift from its own verifier is a protocol split waiting to happen.
/// One definition, both sides.
pub fn registration_challenge(network: &str, github: &str, address: &str, nonce_hex: &str, issued_at_ms: u64) -> Vec<u8> {
    registration_challenge_for(network, github, address, nonce_hex, issued_at_ms, LedgerAttribution::Github)
}

/// The canonical ledger id for an on-chain address, with NO registration anywhere in the path.
///
/// **2026-08-02 policy change (testnet-22).** Points now accrue to the testnet ADDRESS that did the
/// work, automatically: create an address, use it, and the operator's aggregation credits it. The
/// GitHub-handle path and the whole registration handshake are gone from the points path.
///
/// What that removes is the failure mode registration actually produced in practice — someone earns
/// on-chain, never registers, and their facts are silently dropped by the fail-closed membership
/// test. The chain already knows who did the work; requiring them to also tell an off-chain service
/// about it only created a way to lose their points.
///
/// What it costs is deliberate and worth stating plainly: an address is not a person. Two addresses
/// are two participants here even if one human holds both, and nothing links an address to a GitHub
/// identity any more. Anti-sybil is therefore a question for the ALLOCATION rules and the operator's
/// aggregation, not for this function — it must not be smuggled back in as a hidden registration
/// check, which is exactly what made the old path lossy.
///
/// The `addr:` prefix is kept so ids stay self-describing and so ledgers written before this change
/// (which used the same spelling for address-attributed registrations) remain comparable.
pub fn ledger_id_for_address(address: &str) -> Option<String> {
    let address = address.trim();
    // The only gate is "is this a well-formed address for one of our networks". Anything else would
    // be a registration check by another name.
    if address.is_empty() || !address.contains(':') || address.len() > MAX_LEDGER_ADDRESS_LEN {
        return None;
    }
    let (prefix, payload) = address.split_once(':').expect("checked above");
    if !matches!(prefix, "misaka" | "misakatest" | "misakasim" | "misakadev") || payload.is_empty() {
        return None;
    }
    if !payload.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    Some(format!("addr:{address}"))
}

/// Bech32 addresses on these networks are well under this; the bound exists so a malformed fact
/// cannot push an unbounded string into the ledger as an id.
pub const MAX_LEDGER_ADDRESS_LEN: usize = 256;

/// The registration challenge including the participant's [`LedgerAttribution`] choice.
///
/// `Github` reproduces the **v1** bytes EXACTLY — byte-identical to what [`registration_challenge`]
/// has always produced — so every already-issued invitation, already-signed request and stored
/// signature keeps verifying unchanged. Choosing `Address` selects a distinct **v2** message that
/// names the choice, which is what makes the choice unforgeable: a v2 signature cannot be replayed
/// as a v1 registration (different bytes ⇒ different signature), so an operator cannot silently
/// redirect a participant's points, in either direction.
pub fn registration_challenge_for(
    network: &str,
    github: &str,
    address: &str,
    nonce_hex: &str,
    issued_at_ms: u64,
    attribution: LedgerAttribution,
) -> Vec<u8> {
    match attribution {
        LedgerAttribution::Github => format!(
            "MISAKA-TESTNET-POINTS-REGISTRATION v1\nnetwork: {network}\ngithub: {github}\naddress: {address}\nnonce: {nonce_hex}\nissued_at: {issued_at_ms}"
        )
        .into_bytes(),
        LedgerAttribution::Address => format!(
            "MISAKA-TESTNET-POINTS-REGISTRATION v2\nnetwork: {network}\ngithub: {github}\naddress: {address}\nnonce: {nonce_hex}\nissued_at: {issued_at_ms}\nattribution: address"
        )
        .into_bytes(),
    }
}

/// Verify a registration: the address binds the pubkey, and the ML-DSA-87
/// signature verifies over the exact `challenge` bytes (the §11-B registration
/// message) under [`MTP_REGISTER_CONTEXT`]. A wrong length or bad signature is a
/// hard reject (never a panic — `verify_*` returns `Err` on malformed input).
pub fn verify_registration(
    github: &str,
    address_str: &str,
    pubkey: &[u8],
    challenge: &[u8],
    signature: &[u8],
    expected_prefix: Prefix,
) -> Result<Registration, RegistrationError> {
    bind_key_to_address(address_str, pubkey, expected_prefix)?;
    if !matches!(verify_mldsa87_with_context(pubkey, challenge, signature, MTP_REGISTER_CONTEXT), Ok(true)) {
        return Err(RegistrationError::BadSignature);
    }
    Ok(Registration { github: github.to_string(), address: address_str.to_string(), pubkey: pubkey.to_vec() })
}

/// Verify a TGE claim: the same registered `pubkey` signs the `claim` message
/// (§11-B: identity + mainnet_address + total_points_ack + nonce) under
/// [`MTP_CLAIM_CONTEXT`]. Returns true iff the signature is valid.
pub fn verify_claim(pubkey: &[u8], claim: &[u8], signature: &[u8]) -> bool {
    matches!(verify_mldsa87_with_context(pubkey, claim, signature, MTP_CLAIM_CONTEXT), Ok(true))
}

#[cfg(test)]
mod address_attribution_tests {
    use super::*;

    /// 2026-08-02 — using a testnet address IS the enrolment. No registration, no handshake, no
    /// signature: the chain already proved who did the work.
    #[test]
    fn any_well_formed_address_is_its_own_ledger_id() {
        for addr in ["misakatest:qabc123", "misaka:qxyz", "misakadev:q1", "misakasim:q9"] {
            assert_eq!(ledger_id_for_address(addr).as_deref(), Some(format!("addr:{addr}").as_str()));
        }
        // Surrounding whitespace from a hand-edited fact file must not mint a second id.
        assert_eq!(ledger_id_for_address("  misakatest:qabc  ").as_deref(), Some("addr:misakatest:qabc"));
    }

    /// The ONLY gate is well-formedness. Anything stricter would be a registration check wearing a
    /// different name, and re-introduce the silent point loss this change removes.
    #[test]
    fn malformed_input_cannot_invent_an_id() {
        for bad in ["", "   ", "no-colon", "misakatest:", ":qabc", "bitcoin:qabc", "misakatest:has space", "misakatest:has-dash"] {
            assert_eq!(ledger_id_for_address(bad), None, "{bad:?} must not resolve");
        }
        let too_long = format!("misakatest:{}", "q".repeat(MAX_LEDGER_ADDRESS_LEN));
        assert_eq!(ledger_id_for_address(&too_long), None, "an unbounded id must not reach the ledger");
    }

    /// Distinct addresses are distinct participants. Stated as a test because it is the trade the
    /// address policy makes: one human with two addresses is two ids, and re-linking them is an
    /// allocation-time decision, never a hidden lookup here.
    #[test]
    fn distinct_addresses_are_distinct_ids() {
        assert_ne!(ledger_id_for_address("misakatest:qalice"), ledger_id_for_address("misakatest:qbob"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_pq_validator_core::ValidatorKey;

    // Derive a real misakatest address from a real ML-DSA-87 key.
    fn key_and_addr(seed: u8) -> (ValidatorKey, Vec<u8>, String) {
        let key = ValidatorKey::from_seed([seed; 32]);
        let pk = key.public_key().to_vec();
        let payload = blake2b_512_address_payload(&pk);
        let addr = Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &payload.as_bytes());
        (key, pk, addr.to_string())
    }

    #[test]
    fn valid_registration_round_trips() {
        let (key, pk, addr) = key_and_addr(0x21);
        let challenge = b"MISAKA-TESTNET-POINTS-REGISTRATION v1\ngithub: alice\nnonce: ab..";
        let sig = key.sign_with_context(challenge, MTP_REGISTER_CONTEXT);
        let reg = verify_registration("alice", &addr, &pk, challenge, &sig, Prefix::Testnet).unwrap();
        assert_eq!(reg.github, "alice");
        assert_eq!(reg.pubkey, pk);
    }

    #[test]
    fn tampered_challenge_or_wrong_key_is_rejected() {
        let (key, pk, addr) = key_and_addr(0x22);
        let challenge = b"MISAKA-TESTNET-POINTS-REGISTRATION v1\ngithub: bob";
        let sig = key.sign_with_context(challenge, MTP_REGISTER_CONTEXT);
        // tampered message
        assert_eq!(
            verify_registration("bob", &addr, &pk, b"different message", &sig, Prefix::Testnet),
            Err(RegistrationError::BadSignature)
        );
        // a different key whose address does not match
        let (_k2, pk2, _a2) = key_and_addr(0x33);
        assert_eq!(
            verify_registration("bob", &addr, &pk2, challenge, &sig, Prefix::Testnet),
            Err(RegistrationError::KeyAddressMismatch)
        );
        // wrong context (signed under claim, verified as register) fails
        let sig_claim = key.sign_with_context(challenge, MTP_CLAIM_CONTEXT);
        assert_eq!(
            verify_registration("bob", &addr, &pk, challenge, &sig_claim, Prefix::Testnet),
            Err(RegistrationError::BadSignature)
        );
    }

    #[test]
    fn claim_round_trips_and_rejects_tamper_and_wrong_context() {
        // §5 test 6 (ADR-0038 D8): the TGE claim path — the registered key signs the
        // claim message under MTP_CLAIM_CONTEXT. Closes verify_claim's zero coverage.
        let (key, pk, _addr) = key_and_addr(0x51);
        // §11-B claim message: identity + mainnet_address + total_points_ack + nonce.
        let claim = b"MISAKA-TESTNET-POINTS-CLAIM v1\nid: gh:alice\nmainnet: misaka:xyz\ntotal_points_ack: 1234000\nnonce: 00ff";
        let sig = key.sign_with_context(claim, MTP_CLAIM_CONTEXT);
        assert!(verify_claim(&pk, claim, &sig), "a correct claim signature must verify");

        // tampered message → reject.
        assert!(!verify_claim(&pk, b"MISAKA-TESTNET-POINTS-CLAIM v1\nid: gh:mallory", &sig));
        // a different key → reject.
        let (_k2, pk2, _a2) = key_and_addr(0x52);
        assert!(!verify_claim(&pk2, claim, &sig));
        // a signature made under the REGISTER context must NOT verify as a claim
        // (cross-context domain separation, D7).
        let sig_register = key.sign_with_context(claim, MTP_REGISTER_CONTEXT);
        assert!(!verify_claim(&pk, claim, &sig_register), "register-context sig is not a valid claim");
        // a malformed (short) signature is a hard reject, never a panic.
        assert!(!verify_claim(&pk, claim, &[0u8; 10]));
    }

    #[test]
    fn mainnet_address_prefix_is_rejected_for_testnet_registration() {
        let (key, pk, _) = key_and_addr(0x24);
        let payload = blake2b_512_address_payload(&pk);
        let mainnet_addr = Address::new(Prefix::Mainnet, Version::PubKeyHashMlDsa87, &payload.as_bytes()).to_string();
        let challenge = b"x";
        let sig = key.sign_with_context(challenge, MTP_REGISTER_CONTEXT);
        assert_eq!(
            verify_registration("eve", &mainnet_addr, &pk, challenge, &sig, Prefix::Testnet),
            Err(RegistrationError::WrongPrefix)
        );
    }
}
